//! One SQLite store per ecosystem, in WAL mode, in the offline folder. An install replaces the
//! rows inside one transaction, which SQLite makes atomic even across a crash; a check reads
//! inside one read transaction, which sees one snapshot. Every file a store has is named from
//! its `Ecosystem` value, and no folder is ever listed.
//!
//! Each store has a lock file that checks hold shared while they read and that a removal or a
//! rebuild holds exclusively, so a store is never deleted while a reader has it open.

use crate::ecosystem::Ecosystem;
use crate::offline::cleaned;
use crate::offline::locks::{self, LockFailure};
use chrono::{DateTime, Utc};
use common_file_utils::permissions::open_owner_only;
use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension, params};
use std::fs::{File, OpenOptions};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// The store layout this version writes and reads.
pub(crate) const FORMAT: i64 = 1;

/// The tables, created inside the first install's transaction.
pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  format INTEGER NOT NULL,
  etag TEXT,
  archive_bytes INTEGER NOT NULL,
  records INTEGER NOT NULL,
  downloaded_at TEXT NOT NULL,
  current_at TEXT NOT NULL,
  attempted_at TEXT,
  attempt_failure TEXT,
  CHECK ((attempted_at IS NULL) = (attempt_failure IS NULL))
);
CREATE TABLE IF NOT EXISTS records (id TEXT PRIMARY KEY, record TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS packages (name TEXT NOT NULL, id TEXT NOT NULL, PRIMARY KEY (name, id));
";

/// The files of one ecosystem's store.
#[derive(Debug, Clone)]
pub(crate) struct StoreFiles {
    pub db: PathBuf,
    pub wal: PathBuf,
    pub shm: PathBuf,
    /// The rollback journal a store not in WAL mode can leave.
    pub journal: PathBuf,
    /// The download being written.
    pub part: PathBuf,
    /// Held shared by readers, exclusively by a removal or a rebuild.
    pub lock: PathBuf,
}

impl StoreFiles {
    pub(crate) fn of(dir: &Path, ecosystem: Ecosystem) -> Self {
        let name = ecosystem.name();
        Self {
            db: dir.join(format!("{name}.sqlite")),
            wal: dir.join(format!("{name}.sqlite-wal")),
            shm: dir.join(format!("{name}.sqlite-shm")),
            journal: dir.join(format!("{name}.sqlite-journal")),
            part: dir.join(format!("{name}.zip.part")),
            lock: dir.join(format!("{name}.lock")),
        }
    }
}

/// The `format` a rebuild writes as soon as it has created the new store, before its install
/// commits: from then on, a rebuild that fails, or a process that dies during one, leaves a store
/// that reads Unreadable, still downloaded and still rebuildable. Only a failure between
/// deleting the old files and committing this mark leaves no store.
const REBUILDING: i64 = 0;

/// The lock one updater at a time holds, for an update or a removal.
pub(crate) fn update_lock(dir: &Path) -> PathBuf {
    dir.join("update.lock")
}

/// What a store says about its data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreMeta {
    /// The download's `ETag`, sent back as `If-None-Match`.
    pub etag: Option<String>,
    pub archive_bytes: u64,
    pub records: u64,
    /// When the install committed.
    pub downloaded_at: DateTime<Utc>,
    /// The last time the copy matched the server: when the download it came from, or a later
    /// answer that it had not changed, was asked for.
    pub current_at: DateTime<Utc>,
    /// The last update that failed, cleared by a success.
    pub last_failure: Option<FailedAttempt>,
}

/// An update that failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedAttempt {
    pub at: DateTime<Utc>,
    /// The failure's words, as stored; clean them before they are shown.
    pub failure: String,
}

/// One ecosystem's offline data as a read finds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataStatus {
    /// No store, or one an install never finished (no `meta` row).
    NotDownloaded,
    Ready {
        meta: StoreMeta,
        /// The store's files, the WAL included.
        file_bytes: u64,
    },
    /// Not readable right now: busy, held by another program, or a file error that says
    /// nothing about the data. Nothing acts on it but to try again later.
    Unavailable { reason: String },
    /// A removal or a rebuild holds the store.
    Locked,
    /// Definitely not usable: not a database, corrupt, not in WAL mode, or format 0.
    Unreadable { reason: String },
    /// Saved by a newer dep-check.
    NewerFormat,
}

/// A store opened for reading, inside one read transaction, with its lock held shared.
pub(crate) struct Reader {
    // Dropped in this order: the connection closes before the lock is released.
    conn: Connection,
    meta: StoreMeta,
    file_bytes: u64,
    _lock: File,
}

/// What opening a store for reading found.
pub(crate) enum OpenRead {
    Opened(Box<Reader>),
    NotDownloaded,
    Locked,
    Unavailable(String),
    Unreadable(String),
    NewerFormat,
}

/// The status of `ecosystem`'s data in `dir`, read under its lock like a check's read.
pub fn status(dir: &Path, ecosystem: Ecosystem) -> DataStatus {
    match open_read(dir, ecosystem) {
        OpenRead::Opened(reader) => DataStatus::Ready {
            meta: reader.meta.clone(),
            file_bytes: reader.file_bytes,
        },
        OpenRead::NotDownloaded => DataStatus::NotDownloaded,
        OpenRead::Locked => DataStatus::Locked,
        OpenRead::Unavailable(reason) => DataStatus::Unavailable { reason },
        OpenRead::Unreadable(reason) => DataStatus::Unreadable { reason },
        OpenRead::NewerFormat => DataStatus::NewerFormat,
    }
}

/// `ecosystem`'s store opened for reading: its lock taken shared, a read-only query mode set,
/// and one read transaction begun, so every read sees one snapshot.
pub(crate) fn open_read(dir: &Path, ecosystem: Ecosystem) -> OpenRead {
    match std::fs::symlink_metadata(dir) {
        Ok(_) => {}
        Err(error) if is_absent(&error) => return OpenRead::NotDownloaded,
        Err(error) => return OpenRead::Unavailable(cleaned(&error.to_string())),
    }
    let files = StoreFiles::of(dir, ecosystem);
    let lock = match locks::try_shared(&files.lock) {
        Ok(lock) => lock,
        Err(LockFailure::Held) => return OpenRead::Locked,
        Err(LockFailure::Failed(reason)) => return OpenRead::Unavailable(reason),
    };
    match std::fs::symlink_metadata(&files.db) {
        Ok(_) => {}
        Err(error) if is_absent(&error) => return OpenRead::NotDownloaded,
        Err(error) => return OpenRead::Unavailable(cleaned(&error.to_string())),
    }
    match read_meta(&files) {
        Ok(Opened::Ready(conn, meta)) => match file_bytes(&files) {
            Ok(file_bytes) => OpenRead::Opened(Box::new(Reader {
                conn,
                meta,
                file_bytes,
                _lock: lock,
            })),
            Err(error) => OpenRead::Unavailable(cleaned(&error.to_string())),
        },
        Ok(Opened::NotDownloaded) => OpenRead::NotDownloaded,
        Ok(Opened::NewerFormat) => OpenRead::NewerFormat,
        Ok(Opened::Unreadable(reason)) => OpenRead::Unreadable(reason),
        Err(error) => verdict(&error),
    }
}

enum Opened {
    Ready(Connection, StoreMeta),
    NotDownloaded,
    NewerFormat,
    Unreadable(String),
}

fn read_meta(files: &StoreFiles) -> rusqlite::Result<Opened> {
    let conn = connection(&files.db)?;
    // Writes refused on this connection. It is opened read-write anyway: after a killed
    // install the WAL needs recovery, which a read-only connection cannot do.
    conn.pragma_update(None, "query_only", true)?;
    conn.execute_batch("BEGIN")?;
    let has_meta: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'meta'",
        [],
        |row| row.get(0),
    )?;
    if has_meta == 0 {
        return Ok(Opened::NotDownloaded);
    }
    let Some(format) = conn
        .query_row("SELECT format FROM meta WHERE id = 1", [], |row| {
            row.get::<_, i64>(0)
        })
        .optional()?
    else {
        return Ok(Opened::NotDownloaded);
    };
    if format > FORMAT {
        return Ok(Opened::NewerFormat);
    }
    if format == REBUILDING {
        return Ok(Opened::Unreadable(
            "the data is being rebuilt, or a rebuild did not finish".to_owned(),
        ));
    }
    if format < FORMAT {
        return Ok(Opened::Unreadable(format!("the data has format {format}")));
    }
    let journal: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    if !journal.eq_ignore_ascii_case("wal") {
        return Ok(Opened::Unreadable(
            "the data is not in SQLite's WAL mode".to_owned(),
        ));
    }
    let row = conn.query_row(
        "SELECT etag, archive_bytes, records, downloaded_at, current_at, attempted_at, attempt_failure
         FROM meta WHERE id = 1",
        [],
        |row| {
            Ok(MetaRow {
                etag: row.get(0)?,
                archive_bytes: row.get(1)?,
                records: row.get(2)?,
                downloaded_at: row.get(3)?,
                current_at: row.get(4)?,
                attempted_at: row.get(5)?,
                attempt_failure: row.get(6)?,
            })
        },
    )?;
    Ok(match row.meta() {
        Some(meta) => Opened::Ready(conn, meta),
        None => Opened::Unreadable("the data's description cannot be read".to_owned()),
    })
}

struct MetaRow {
    etag: Option<String>,
    archive_bytes: i64,
    records: i64,
    downloaded_at: String,
    current_at: String,
    attempted_at: Option<String>,
    attempt_failure: Option<String>,
}

impl MetaRow {
    /// The row as meta, or `None` when a count is negative or a date does not parse.
    fn meta(self) -> Option<StoreMeta> {
        let last_failure = match (self.attempted_at, self.attempt_failure) {
            (Some(at), Some(failure)) => Some(FailedAttempt {
                at: parse_time(&at)?,
                failure,
            }),
            (None, None) => None,
            (Some(_), None) | (None, Some(_)) => return None,
        };
        Some(StoreMeta {
            etag: self.etag,
            archive_bytes: u64::try_from(self.archive_bytes).ok()?,
            records: u64::try_from(self.records).ok()?,
            downloaded_at: parse_time(&self.downloaded_at)?,
            current_at: parse_time(&self.current_at)?,
            last_failure,
        })
    }
}

fn parse_time(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

/// What an SQLite error means for a read. "Not a database", "corrupt", a plain SQL error (from
/// this module's fixed queries, a schema that is not this store's), and a value of a type this
/// store never writes are definite. Busy, locked, a file another program holds, a permission or
/// disk error, and errors about this program or the host say nothing about the data, so none of
/// them may lead to deleting it.
fn verdict(error: &rusqlite::Error) -> OpenRead {
    let reason = cleaned(&error.to_string());
    if let Some(code) = error.sqlite_error_code() {
        return match code {
            ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt | ErrorCode::Unknown => {
                OpenRead::Unreadable(reason)
            }
            // `ErrorCode` is non-exhaustive; every other code, a future one included, is kept
            // as not definite.
            _ => OpenRead::Unavailable(reason),
        };
    }
    match error {
        rusqlite::Error::InvalidColumnType(..)
        | rusqlite::Error::FromSqlConversionFailure(..)
        | rusqlite::Error::IntegralValueOutOfRange(..)
        | rusqlite::Error::Utf8Error(..) => OpenRead::Unreadable(reason),
        // `rusqlite::Error` is non-exhaustive; every other error, a future one included, is
        // kept as not definite.
        _ => OpenRead::Unavailable(reason),
    }
}

impl Reader {
    /// What the store says about its data.
    pub(crate) fn meta(&self) -> &StoreMeta {
        &self.meta
    }

    /// The stored records the index lists for the normalized package `name`. A listed id with
    /// no record is `None`: the join is a left join so such a row is not silently dropped.
    pub(crate) fn records_for(&self, name: &str) -> rusqlite::Result<Vec<Option<String>>> {
        let mut statement = self.conn.prepare_cached(
            "SELECT r.record FROM packages p LEFT JOIN records r ON r.id = p.id
             WHERE p.name = ?1 ORDER BY p.id",
        )?;
        let rows = statement.query_map([name], |row| row.get::<_, Option<String>>(0))?;
        rows.collect()
    }
}

/// A connection to the store at `db`: read-write, never creating the file ("the database must
/// already exist"), and refusing a store file that is a link. SQLite's no-follow flag refuses a
/// link anywhere in the path, so the store is opened under its folder's real path: a home
/// reached through a link (macOS's `/var`, `/home` on Fedora Silverblue) opens, and a link at the
/// store file itself is still refused.
pub(crate) fn connection(db: &Path) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(
        in_real_folder(db),
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NOFOLLOW
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
}

/// `db` under its folder's real path on Unix. Elsewhere, and when the folder cannot be resolved,
/// `db` as given, so the open answers with SQLite's own error. Windows paths stay as given since
/// a resolved one carries the `\\?\` prefix into every reason that names the file.
fn in_real_folder(db: &Path) -> PathBuf {
    if !cfg!(unix) {
        return db.to_path_buf();
    }
    db.parent()
        .zip(db.file_name())
        .and_then(|(folder, name)| folder.canonicalize().ok().map(|real| real.join(name)))
        .unwrap_or_else(|| db.to_path_buf())
}

/// Puts `conn`'s store in WAL mode; `false` when SQLite answers with another mode, as it does
/// when it cannot change it (WAL "does not work over a network filesystem").
pub(crate) fn set_wal(conn: &Connection) -> rusqlite::Result<bool> {
    let mode: String =
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
    Ok(mode.eq_ignore_ascii_case("wal"))
}

/// Creates an empty store file. The caller holds the store's lock exclusively. A leftover WAL,
/// shared-memory file, or rollback journal is deleted first, since a new database beside an old
/// journal can be paired with it; the file is created new, so a link planted at its path is
/// refused.
pub(crate) fn create(files: &StoreFiles) -> io::Result<()> {
    remove_if_present(&files.wal)?;
    remove_if_present(&files.shm)?;
    remove_if_present(&files.journal)?;
    open_owner_only(OpenOptions::new().write(true).create_new(true), &files.db).map(drop)
}

/// Marks a store that is being rebuilt, in one transaction: the tables, and a `meta` row with
/// the rebuilding format, which the install's own `meta` row replaces when it commits.
pub(crate) fn mark_rebuilding(conn: &mut Connection, now: DateTime<Utc>) -> rusqlite::Result<()> {
    let transaction = conn.transaction()?;
    transaction.execute_batch(SCHEMA)?;
    transaction.execute(
        "INSERT OR REPLACE INTO meta (id, format, archive_bytes, records, downloaded_at, current_at)
         VALUES (1, ?1, 0, 0, ?2, ?2)",
        params![REBUILDING, now.to_rfc3339()],
    )?;
    transaction.commit()
}

/// Deletes the store's database, then its WAL, shared-memory file, and rollback journal, and
/// returns the bytes they held. The caller holds the store's lock exclusively.
pub(crate) fn delete_store(files: &StoreFiles) -> io::Result<u64> {
    let mut bytes = remove_if_present(&files.db)?;
    for companion in [&files.wal, &files.shm, &files.journal] {
        bytes = bytes.saturating_add(remove_if_present(companion)?);
    }
    Ok(bytes)
}

/// Deletes `path` when it is there, a link itself rather than its target, and returns its size.
pub(crate) fn remove_if_present(path: &Path) -> io::Result<u64> {
    let size = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(error) if is_absent(&error) => return Ok(0),
        Err(error) => return Err(error),
    };
    match std::fs::remove_file(path) {
        Ok(()) => Ok(size),
        Err(error) if is_absent(&error) => Ok(0),
        Err(error) => Err(error),
    }
}

/// The bytes the store's database and its WAL and shared-memory files hold; a file that is not
/// there holds none.
fn file_bytes(files: &StoreFiles) -> io::Result<u64> {
    let mut bytes: u64 = 0;
    for path in [&files.db, &files.wal, &files.shm] {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => bytes = bytes.saturating_add(metadata.len()),
            Err(error) if is_absent(&error) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(bytes)
}

/// Whether any of `paths` is there.
fn any_present(paths: &[&PathBuf]) -> io::Result<bool> {
    for path in paths {
        match std::fs::symlink_metadata(path) {
            Ok(_) => return Ok(true),
            Err(error) if is_absent(&error) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

/// Whether `error` says the path is not there. A path through a regular file cannot exist
/// either, and only Windows reports that as `NotFound`.
pub(crate) fn is_absent(error: &io::Error) -> bool {
    matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory)
}

/// How long a note in `meta` waits on a busy store: only an updater writes, under the update
/// lock, so a wait means something is wrong and the note is not worth holding the run for.
const NOTE_BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

/// Marks the data at `db` as matching the server at `now`, and clears the last failed attempt.
pub(crate) fn write_current(db: &Path, now: DateTime<Utc>) -> rusqlite::Result<()> {
    let conn = connection(db)?;
    conn.busy_timeout(NOTE_BUSY_TIMEOUT)?;
    conn.execute(
        "UPDATE meta SET current_at = ?1, attempted_at = NULL, attempt_failure = NULL WHERE id = 1",
        params![now.to_rfc3339()],
    )
    .map(drop)
}

/// Notes a failed update at `now` with its words in the data at `db`.
pub(crate) fn write_attempt(db: &Path, now: DateTime<Utc>, failure: &str) -> rusqlite::Result<()> {
    let conn = connection(db)?;
    conn.busy_timeout(NOTE_BUSY_TIMEOUT)?;
    conn.execute(
        "UPDATE meta SET attempted_at = ?1, attempt_failure = ?2 WHERE id = 1",
        params![now.to_rfc3339(), failure],
    )
    .map(drop)
}

/// What [`remove`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveOutcome {
    /// The files were deleted; `bytes` is what they held.
    Removed { bytes: u64 },
    /// None of the store's files was there.
    NotDownloaded,
    /// An update runs.
    Busy,
    /// A check reads the data, or a rebuild holds it.
    InUse,
    /// A file could not be deleted; the cleaned reason.
    Failed(String),
}

/// Deletes `ecosystem`'s files in `dir`: the database first, then its WAL, shared-memory file,
/// rollback journal, and a leftover download, whichever are there, so the leftovers of an
/// install that never finished go too. It holds the update lock and the store's lock
/// exclusively, and touches only those named files.
pub fn remove(dir: &Path, ecosystem: Ecosystem) -> RemoveOutcome {
    let outcome = remove_files(dir, ecosystem);
    let name = ecosystem.name();
    match &outcome {
        RemoveOutcome::Removed { bytes } => {
            info!(ecosystem = name, bytes, "offline data removed");
        }
        RemoveOutcome::NotDownloaded => {
            info!(
                ecosystem = name,
                outcome = "not_downloaded",
                "offline data not removed"
            );
        }
        RemoveOutcome::Busy => {
            info!(
                ecosystem = name,
                outcome = "busy",
                "offline data not removed"
            );
        }
        RemoveOutcome::InUse => {
            info!(
                ecosystem = name,
                outcome = "in_use",
                "offline data not removed"
            );
        }
        RemoveOutcome::Failed(reason) => {
            warn!(ecosystem = name, outcome = "failed", reason = %reason, "offline data not removed");
        }
    }
    outcome
}

fn remove_files(dir: &Path, ecosystem: Ecosystem) -> RemoveOutcome {
    match std::fs::symlink_metadata(dir) {
        Ok(_) => {}
        Err(error) if is_absent(&error) => return RemoveOutcome::NotDownloaded,
        Err(error) => return RemoveOutcome::Failed(cleaned(&error.to_string())),
    }
    let _updating = match locks::try_exclusive(&update_lock(dir)) {
        Ok(lock) => lock,
        Err(LockFailure::Held) => return RemoveOutcome::Busy,
        Err(LockFailure::Failed(reason)) => return RemoveOutcome::Failed(reason),
    };
    let files = StoreFiles::of(dir, ecosystem);
    let _store = match locks::try_exclusive(&files.lock) {
        Ok(lock) => lock,
        Err(LockFailure::Held) => return RemoveOutcome::InUse,
        Err(LockFailure::Failed(reason)) => return RemoveOutcome::Failed(reason),
    };
    let paths = [
        &files.db,
        &files.wal,
        &files.shm,
        &files.journal,
        &files.part,
    ];
    match any_present(&paths) {
        Ok(true) => {}
        Ok(false) => return RemoveOutcome::NotDownloaded,
        Err(error) => return RemoveOutcome::Failed(cleaned(&error.to_string())),
    }
    let deleted = delete_store(&files)
        .and_then(|bytes| remove_if_present(&files.part).map(|part| bytes.saturating_add(part)));
    match deleted {
        Ok(bytes) => RemoveOutcome::Removed { bytes },
        Err(error) => RemoveOutcome::Failed(cleaned(&error.to_string())),
    }
}

#[cfg(test)]
mod tests;
