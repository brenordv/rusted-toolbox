//! The install: every record of the downloaded zip, read into memory and checked, replaces the
//! store's rows inside one transaction. Entry names are never used as paths; nothing is
//! extracted to disk.

use super::{ProgressFn, Stop, Throttle, UpdateFailure, UpdateProgress};
use crate::ecosystem::Ecosystem;
use crate::engine::Clock;
use crate::offline::store::{self, FORMAT, SCHEMA};
use crate::offline::{cleaned, cleaned_id};
use crate::sources::osv::OsvAffected;
use chrono::{DateTime, Utc};
use rusqlite::config::DbConfig;
use rusqlite::{Connection, TransactionBehavior, params};
use serde::Deserialize;
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tracing::debug;
use zip::ZipArchive;
use zip::result::ZipError;

/// Records between two looks at `cancel` and the deadline.
const STOP_CHECK_EVERY: usize = 1000;
/// How long the checkpoint after an install waits on a reader before it gives up.
const CHECKPOINT_BUSY_TIMEOUT: Duration = Duration::from_millis(200);

/// One install's inputs.
pub(super) struct Job {
    /// The complete download, read back through this handle (never reopened by path).
    pub file: File,
    pub install: Install,
    /// The update lock, held until this job ends, so no removal or other update can start
    /// while it writes the store, even if the update that started it is gone.
    pub update_lock: Arc<File>,
}

/// What an install needs besides the download.
pub(super) struct Install {
    pub archive_bytes: u64,
    pub db: PathBuf,
    pub ecosystem: Ecosystem,
    pub etag: Option<String>,
    /// When the download was asked for: the time the copy last matched the server.
    pub current_at: DateTime<Utc>,
    /// Read at the commit, for `downloaded_at`.
    pub clock: Clock,
    pub max_entry_bytes: u64,
    pub progress: ProgressFn,
    pub cancel: Arc<AtomicBool>,
    pub deadline: Option<Instant>,
}

/// Installs the download and returns the records stored. Any end but the commit rolls the
/// transaction back.
pub(super) fn install(job: Job) -> Result<u64, Stop> {
    let Job {
        mut file,
        install,
        update_lock,
    } = job;
    let mut conn = store::connection(&install.db).map_err(|error| UpdateFailure::store(&error))?;
    match store::set_wal(&conn) {
        Ok(true) => {}
        Ok(false) => return Err(Stop::Failed(super::wal_refused())),
        Err(error) => return Err(Stop::Failed(UpdateFailure::store(&error))),
    }
    // Closing the last connection would fold the whole WAL back before this job returns, which
    // delays `Ending` by the length of a checkpoint; the checkpoint after `Ending` does it.
    if let Err(error) = conn.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true) {
        // Best effort: without it the close checkpoints, and `Ending` only comes later.
        debug!(error = %cleaned(&error.to_string()), "the checkpoint on close was left on");
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| UpdateFailure::disk(&error))?;
    let mut archive = ZipArchive::new(file).map_err(|error| bad_archive(&error))?;
    let stored = fill(&mut conn, &mut archive, &install);
    drop(conn);
    drop(update_lock);
    stored
}

/// Replaces every row inside one `BEGIN IMMEDIATE` transaction that ends by writing `meta`.
/// Dropping the transaction on an early return rolls it back.
fn fill(conn: &mut Connection, archive: &mut ZipArchive<File>, job: &Install) -> Result<u64, Stop> {
    let store_error = |error: rusqlite::Error| Stop::Failed(UpdateFailure::store(&error));
    let transaction = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(store_error)?;
    transaction.execute_batch(SCHEMA).map_err(store_error)?;
    transaction
        .execute_batch("DELETE FROM packages; DELETE FROM records;")
        .map_err(store_error)?;
    let total = archive.len();
    let mut stored: u64 = 0;
    {
        let mut insert_record = transaction
            .prepare("INSERT OR REPLACE INTO records (id, record) VALUES (?1, ?2)")
            .map_err(store_error)?;
        let mut insert_name = transaction
            .prepare("INSERT OR IGNORE INTO packages (name, id) VALUES (?1, ?2)")
            .map_err(store_error)?;
        let mut throttle = Throttle::default();
        (job.progress)(UpdateProgress::Installing {
            done: 0,
            total: total as u64,
        });
        throttle.ready(Instant::now());
        for index in 0..total {
            if index % STOP_CHECK_EVERY == 0 {
                stop_check(job)?;
            }
            if let Some(bytes) = read_entry(archive, index, job.max_entry_bytes)?
                && let Some(record) = slim(job.ecosystem, &bytes.name_id, &bytes.data)?
            {
                insert_record
                    .execute(params![record.id, record.json])
                    .map_err(store_error)?;
                for name in &record.names {
                    insert_name
                        .execute(params![name, record.id])
                        .map_err(store_error)?;
                }
                stored += 1;
            }
            let done = index + 1;
            if done < total && throttle.ready(Instant::now()) {
                (job.progress)(UpdateProgress::Installing {
                    done: done as u64,
                    total: total as u64,
                });
            }
        }
        (job.progress)(UpdateProgress::Installing {
            done: total as u64,
            total: total as u64,
        });
    }
    stop_check(job)?;
    if stored == 0 {
        return Err(Stop::Failed(UpdateFailure::NoRecords));
    }
    transaction
        .execute(
            "INSERT OR REPLACE INTO meta
               (id, format, etag, archive_bytes, records, downloaded_at, current_at, attempted_at, attempt_failure)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL)",
            params![
                FORMAT,
                job.etag,
                i64::try_from(job.archive_bytes).unwrap_or(i64::MAX),
                i64::try_from(stored).unwrap_or(i64::MAX),
                (job.clock)().to_rfc3339(),
                job.current_at.to_rfc3339(),
            ],
        )
        .map_err(store_error)?;
    transaction.commit().map_err(store_error)?;
    Ok(stored)
}

/// `Cancelled` once `cancel` is set, `Deadline` once an automatic update's deadline passed.
fn stop_check(job: &Install) -> Result<(), Stop> {
    if job.cancel.load(Ordering::SeqCst) {
        return Err(Stop::Cancelled);
    }
    if job
        .deadline
        .is_some_and(|deadline| Instant::now() >= deadline)
    {
        return Err(Stop::Failed(UpdateFailure::Deadline));
    }
    Ok(())
}

/// One entry's bytes and the id its name gives, for the failure words before its JSON is read.
struct Entry {
    name_id: String,
    data: Vec<u8>,
}

/// The entry at `index`: `None` for a folder or a file that is not `<ID>.json`. Its declared
/// size is checked against the cap, and it is read through a cap one byte over, so a header that
/// understates the size still stops there.
fn read_entry(
    archive: &mut ZipArchive<File>,
    index: usize,
    max_entry_bytes: u64,
) -> Result<Option<Entry>, Stop> {
    let mut entry = archive
        .by_index(index)
        .map_err(|error| bad_archive(&error))?;
    if entry.is_dir() {
        return Ok(None);
    }
    let name = entry
        .name()
        .map_err(|error| bad_archive(&error))?
        .into_owned();
    let Some(stem) = name.strip_suffix(".json") else {
        return Ok(None);
    };
    let name_id = cleaned_id(stem.rsplit('/').next().unwrap_or(stem));
    if entry.size() > max_entry_bytes {
        return Err(Stop::Failed(UpdateFailure::EntryTooLarge { id: name_id }));
    }
    let mut data = Vec::new();
    entry
        .by_ref()
        .take(max_entry_bytes.saturating_add(1))
        .read_to_end(&mut data)
        .map_err(|_| UpdateFailure::Damaged)?;
    if data.len() as u64 > max_entry_bytes {
        return Err(Stop::Failed(UpdateFailure::EntryTooLarge { id: name_id }));
    }
    Ok(Some(Entry { name_id, data }))
}

/// A record as it is stored.
struct Slimmed {
    id: String,
    json: String,
    /// The normalized names of the packages its entries for this ecosystem name.
    names: Vec<String>,
}

/// The record in `data`, slimmed: no `details`, no `credits`, and only the `affected` entries
/// for `ecosystem`. `None` for a withdrawn record or one with no entry for the ecosystem. Only
/// `id` and `affected` are read strictly; the rest is kept as JSON for the report.
fn slim(ecosystem: Ecosystem, name_id: &str, data: &[u8]) -> Result<Option<Slimmed>, Stop> {
    let bad_record = |id: &str| Stop::Failed(UpdateFailure::BadRecord { id: cleaned_id(id) });
    let mut value: Value = serde_json::from_slice(data).map_err(|_| bad_record(name_id))?;
    let Some(object) = value.as_object_mut() else {
        return Err(bad_record(name_id));
    };
    let Some(id) = object.get("id").and_then(Value::as_str).map(str::to_owned) else {
        return Err(bad_record(name_id));
    };
    if object
        .get("withdrawn")
        .is_some_and(|withdrawn| !withdrawn.is_null())
    {
        return Ok(None);
    }
    let affected: Vec<OsvAffected> = match object.get("affected") {
        None | Some(Value::Null) => Vec::new(),
        Some(entries) => Vec::<OsvAffected>::deserialize(entries).map_err(|_| bad_record(&id))?,
    };
    let keep: Vec<bool> = affected
        .iter()
        .map(|entry| {
            entry
                .package
                .as_ref()
                .and_then(|package| package.ecosystem.as_deref())
                == Some(ecosystem.osv_name())
        })
        .collect();
    if !keep.contains(&true) {
        return Ok(None);
    }
    let mut names: Vec<String> = affected
        .iter()
        .zip(&keep)
        .filter(|(_, kept)| **kept)
        .filter_map(|(entry, _)| entry.package.as_ref()?.name.as_deref())
        .map(|name| ecosystem.normalize_name(name))
        .collect();
    names.sort();
    names.dedup();
    object.remove("details");
    object.remove("credits");
    if let Some(Value::Array(entries)) = object.get_mut("affected") {
        let mut kept = keep.iter();
        entries.retain(|_| kept.next().copied().unwrap_or(false));
    }
    let json = serde_json::to_string(&value).map_err(|_| bad_record(&id))?;
    Ok(Some(Slimmed { id, json, names }))
}

fn bad_archive(error: &ZipError) -> Stop {
    match error {
        // A read of the download failed: its bytes are not what was written.
        ZipError::Io(_) => Stop::Failed(UpdateFailure::Damaged),
        other => Stop::Failed(UpdateFailure::BadArchive(cleaned(&other.to_string()))),
    }
}

/// Best effort, after the commit: folds the WAL of the store at `db` back into it. A reader
/// still holding an older snapshot blocks it, which SQLite reports as a 1 in the first column.
pub(super) fn checkpoint(db: &Path) {
    let opened = store::connection(db).and_then(|conn| {
        conn.busy_timeout(CHECKPOINT_BUSY_TIMEOUT)?;
        Ok(conn)
    });
    let conn = match opened {
        Ok(conn) => conn,
        Err(error) => {
            debug!(error = %cleaned(&error.to_string()), "the checkpoint after the install was not run");
            return;
        }
    };
    match conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
        row.get::<_, i64>(0)
    }) {
        Ok(0) => debug!("the checkpoint after the install finished"),
        Ok(_) => debug!("the checkpoint after the install was blocked by a reader"),
        Err(error) => {
            debug!(error = %cleaned(&error.to_string()), "the checkpoint after the install failed");
        }
    }
}
