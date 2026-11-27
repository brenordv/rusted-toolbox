//! Downloads an ecosystem's OSV dump and installs it into its store, with progress. One
//! updater runs at a time per user (`update.lock`); an install replaces the rows inside one
//! transaction, so a failed or cancelled update leaves the data that was there.

use crate::ecosystem::Ecosystem;
use crate::engine::{Clock, system_clock};
use crate::http::{SourceFailure, elapsed_ms, encode_segment};
use crate::offline::due::{self, Due, Skip};
use crate::offline::locks::{self, LockFailure};
use crate::offline::store::{self, DataStatus, StoreFiles, StoreMeta};
use crate::offline::{cleaned, offline_dir};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use common_file_utils::permissions::create_dir_all_owner_only;
use reqwest::Client;
use reqwest::redirect::Policy;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};
use url::Url;

mod download;
mod failure;
mod install;

pub use failure::UpdateFailure;

/// Where OSV.dev publishes its dumps: `<base><ecosystem>/all.zip`.
pub const OSV_DATA_BASE: &str = "https://storage.googleapis.com/osv-vulnerabilities/";
/// The host the dumps come from, as failure words name it.
pub const OSV_DATA_HOST: &str = "storage.googleapis.com";

const BYTES_PER_MIB: u64 = 1024 * 1024;
/// Progress events of one kind come at most this often, and once more at the end.
pub const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// The bounds on one update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateLimits {
    /// How long connecting may take.
    pub connect_timeout: Duration,
    /// How long one read may wait; it starts again after every read that returns data.
    pub read_timeout: Duration,
    /// The largest download announced.
    pub max_archive_bytes: u64,
    /// The largest record in the download.
    pub max_entry_bytes: u64,
    /// How long an automatic update may take in all.
    pub auto_deadline: Duration,
}

impl Default for UpdateLimits {
    /// 10 s to connect, 60 s per read, 8 GiB per download, 8 MiB per record, 10 minutes for an
    /// automatic update.
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(60),
            max_archive_bytes: 8 * 1024 * BYTES_PER_MIB,
            max_entry_bytes: 8 * BYTES_PER_MIB,
            auto_deadline: Duration::from_secs(10 * 60),
        }
    }
}

/// What started an update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// The user asked; it has no deadline.
    Manual,
    /// The daily update; it ends at [`UpdateLimits::auto_deadline`].
    Auto,
}

impl Trigger {
    /// The trigger's word, as the log lines write it.
    pub fn name(self) -> &'static str {
        match self {
            Trigger::Manual => "manual",
            Trigger::Auto => "auto",
        }
    }
}

/// Where an update is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateProgress {
    Connecting,
    /// At most every [`PROGRESS_EVERY`], and once when the download is complete.
    Downloading {
        received: u64,
        total: u64,
    },
    /// Records read from the download; at most every [`PROGRESS_EVERY`], and once at the end.
    Installing {
        done: u64,
        total: u64,
    },
    /// The outcome is fixed; sent before the update's finished log line.
    Ending,
}

/// How an update ended.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateOutcome {
    Installed {
        records: u64,
        archive_bytes: u64,
    },
    /// The server's copy has not changed; the data is confirmed current.
    Unchanged,
    /// Another updater holds the lock; nothing was touched.
    Busy,
    /// An automatic update found the data not due, read under the update lock: only Ready data
    /// a day old is ever updated on its own, so no automatic run installs data nobody asked for
    /// or deletes a store to rebuild it. Nothing was touched.
    Skipped(Skip),
    /// Stopped by the caller; `kept` is when the data still in place was last confirmed
    /// current, `None` when there is none.
    Cancelled {
        kept: Option<DateTime<Utc>>,
    },
    Failed(UpdateFailure),
}

/// Why a stage stopped before its end.
#[derive(Debug)]
enum Stop {
    Cancelled,
    Failed(UpdateFailure),
}

impl From<UpdateFailure> for Stop {
    fn from(failure: UpdateFailure) -> Self {
        Stop::Failed(failure)
    }
}

/// The progress callback; an `Arc` so the blocking install can call it.
pub type ProgressFn = Arc<dyn Fn(UpdateProgress) + Send + Sync>;

/// Sends progress events of one kind at most every [`PROGRESS_EVERY`].
#[derive(Debug, Default)]
pub(crate) struct Throttle {
    last: Option<Instant>,
}

impl Throttle {
    /// Whether an event may go out at `now`; when it may, `now` is when the last one went.
    pub(crate) fn ready(&mut self, now: Instant) -> bool {
        let ready = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= PROGRESS_EVERY);
        if ready {
            self.last = Some(now);
        }
        ready
    }
}

/// What the finished log line reports besides the outcome.
#[derive(Debug, Default)]
struct Facts {
    /// The status the run found the data in: `ready`, `not_downloaded`, `unreadable`, ...; empty
    /// for a run that ended before it read one.
    from: &'static str,
    received: u64,
    archive_bytes: u64,
    records: u64,
    download_ms: u64,
    install_ms: u64,
    attempt_write_failed: bool,
}

/// Downloads and installs ecosystems' OSV dumps.
pub struct Updater {
    client: Client,
    base: Url,
    dir: PathBuf,
    clock: Clock,
    limits: UpdateLimits,
}

impl Updater {
    /// The real download address, the stores in the toolbox home, and the default limits.
    /// `user_agent` goes on every request.
    ///
    /// # Errors
    /// When there is no home folder, or the HTTP client cannot be built.
    pub fn new(user_agent: &str) -> Result<Self> {
        let dir = offline_dir().context("there is no home folder to keep offline data in")?;
        let base = Url::parse(OSV_DATA_BASE).context("Failed to read the OSV data address")?;
        Self::with_parts(
            user_agent,
            base,
            dir,
            system_clock(),
            UpdateLimits::default(),
        )
    }

    /// Downloads from `base` (host and folder, ending in `/`) into the stores in `dir`. No flag,
    /// file, or variable sets the address: a download from a host somebody chose could install
    /// data that answers "clean".
    ///
    /// # Errors
    /// When the HTTP client cannot be built, as with a `user_agent` that is not a valid header
    /// value.
    pub fn with_parts(
        user_agent: &str,
        base: Url,
        dir: PathBuf,
        clock: Clock,
        limits: UpdateLimits,
    ) -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(limits.connect_timeout)
            .read_timeout(limits.read_timeout)
            .redirect(Policy::none())
            .user_agent(user_agent)
            .build()
            .context("Failed to build the download client")?;
        Ok(Self {
            client,
            base,
            dir,
            clock,
            limits,
        })
    }

    /// The folder the stores are in.
    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// Updates `ecosystem`'s data: downloads its dump (sending the stored `ETag` so an
    /// unchanged copy costs one answer) and installs it in place of the old rows. Every end but
    /// `Busy` and `Skipped` sends [`UpdateProgress::Ending`] as soon as the outcome is fixed,
    /// then the run logs its one finished line and releases its hold on the update lock. The
    /// install's blocking work holds the lock too, so the lock stays taken until that work ends
    /// even when the caller drops this future.
    ///
    /// Setting `cancel` stops the update between chunks and every 1,000 records, keeping the old
    /// data. A set flag stops every update that reads it, so each run takes a fresh one.
    pub async fn update(
        &self,
        ecosystem: Ecosystem,
        trigger: Trigger,
        progress: ProgressFn,
        cancel: Arc<AtomicBool>,
    ) -> UpdateOutcome {
        info!(
            ecosystem = ecosystem.name(),
            trigger = trigger.name(),
            "offline update started"
        );
        let mut facts = Facts::default();
        if let Err(error) = create_dir_all_owner_only(&self.dir) {
            progress(UpdateProgress::Ending);
            let outcome = UpdateOutcome::Failed(UpdateFailure::disk(&error));
            log_finished(ecosystem, trigger, &outcome, &facts);
            return outcome;
        }
        let lock = match locks::try_exclusive(&store::update_lock(&self.dir)) {
            Ok(lock) => Arc::new(lock),
            Err(LockFailure::Held) => {
                log_finished(ecosystem, trigger, &UpdateOutcome::Busy, &facts);
                return UpdateOutcome::Busy;
            }
            Err(LockFailure::Failed(reason)) => {
                progress(UpdateProgress::Ending);
                let outcome = UpdateOutcome::Failed(UpdateFailure::Lock(reason));
                log_finished(ecosystem, trigger, &outcome, &facts);
                return outcome;
            }
        };
        let run = Run {
            updater: self,
            ecosystem,
            trigger,
            files: StoreFiles::of(&self.dir, ecosystem),
            progress,
            cancel,
            update_lock: Arc::clone(&lock),
        };
        let outcome = run.locked(&mut facts).await;
        log_finished(ecosystem, trigger, &outcome, &facts);
        drop(run);
        drop(lock);
        outcome
    }
}

/// One update, run while it holds the update lock.
struct Run<'a> {
    updater: &'a Updater,
    ecosystem: Ecosystem,
    trigger: Trigger,
    files: StoreFiles,
    progress: ProgressFn,
    cancel: Arc<AtomicBool>,
    /// The update lock, shared with the blocking work that writes the store.
    update_lock: Arc<File>,
}

impl Run<'_> {
    async fn locked(&self, facts: &mut Facts) -> UpdateOutcome {
        // The status read takes and drops the store's shared lock: on Windows a shared lock
        // still held here would make the rebuild's exclusive lock answer "in use".
        let status = store::status(&self.updater.dir, self.ecosystem);
        facts.from = status_word(&status);
        match self.trigger {
            Trigger::Auto => {
                if let Due::Skipped(skip) = due::evaluate(&status, (self.updater.clock)()) {
                    return UpdateOutcome::Skipped(skip);
                }
            }
            Trigger::Manual => {}
        }
        let (ready, rebuild) = match status {
            DataStatus::NotDownloaded => (None, false),
            DataStatus::Ready { meta, .. } => (Some(meta), false),
            DataStatus::Unreadable { .. } => (None, true),
            DataStatus::Unavailable { .. } | DataStatus::Locked => {
                (self.progress)(UpdateProgress::Ending);
                return UpdateOutcome::Failed(UpdateFailure::InUse);
            }
            DataStatus::NewerFormat => {
                (self.progress)(UpdateProgress::Ending);
                return UpdateOutcome::Failed(UpdateFailure::NewerFormat);
            }
        };
        let deadline = match self.trigger {
            Trigger::Manual => None,
            Trigger::Auto => Some(Instant::now() + self.updater.limits.auto_deadline),
        };
        let stopped = match self
            .download_and_install(ready.as_ref(), rebuild, deadline, facts)
            .await
        {
            Ok(outcome) => {
                self.remove_part();
                return outcome;
            }
            Err(stop) => stop,
        };
        (self.progress)(UpdateProgress::Ending);
        self.remove_part();
        match stopped {
            Stop::Cancelled => UpdateOutcome::Cancelled {
                kept: ready.map(|meta| meta.current_at),
            },
            Stop::Failed(failure) => {
                if ready.is_some() {
                    let words = failure.describe(self.ecosystem);
                    if let Err(error) =
                        store::write_attempt(&self.files.db, (self.updater.clock)(), &words)
                    {
                        // Best effort: the note only delays the next automatic update; the
                        // failure itself is the outcome either way.
                        debug!(error = %cleaned(&error.to_string()), "the failed attempt was not noted");
                        facts.attempt_write_failed = true;
                    }
                }
                UpdateOutcome::Failed(failure)
            }
        }
    }

    /// Steps 3 to 8: a fresh `.part`, the download, the store made ready, and the install. Every
    /// end that is not an error has sent `Ending` already.
    async fn download_and_install(
        &self,
        ready: Option<&StoreMeta>,
        rebuild: bool,
        deadline: Option<Instant>,
        facts: &mut Facts,
    ) -> Result<UpdateOutcome, Stop> {
        let part = download::fresh_part(&self.files.part).map_err(|e| UpdateFailure::disk(&e))?;
        let url = archive_url(&self.updater.base, self.ecosystem)?;
        (self.progress)(UpdateProgress::Connecting);
        // The copy matches the server as of the request, not as of the end of a long download.
        let requested_at = (self.updater.clock)();
        let began = Instant::now();
        let fetched = download::fetch(download::Fetch {
            client: &self.updater.client,
            url,
            etag: ready.and_then(|meta| meta.etag.as_deref()),
            limits: &self.updater.limits,
            deadline,
            progress: &self.progress,
            cancel: &self.cancel,
            part,
            received: &mut facts.received,
        })
        .await;
        facts.download_ms = elapsed_ms(began);
        let (file, archive_bytes, etag) = match fetched? {
            download::Fetched::NotModified => {
                (self.progress)(UpdateProgress::Ending);
                if let Err(error) = store::write_current(&self.files.db, requested_at) {
                    // Best effort: the data is current either way; only its date could not be
                    // moved, so the next automatic update comes sooner.
                    debug!(error = %cleaned(&error.to_string()), "the confirmation was not noted");
                    facts.attempt_write_failed = true;
                }
                return Ok(UpdateOutcome::Unchanged);
            }
            download::Fetched::Complete {
                file,
                archive_bytes,
                etag,
            } => (file, archive_bytes, etag),
        };
        facts.archive_bytes = archive_bytes;
        self.prepare_store(rebuild, requested_at)?;
        let began = Instant::now();
        let job = install::Job {
            file,
            install: install::Install {
                archive_bytes,
                db: self.files.db.clone(),
                ecosystem: self.ecosystem,
                etag,
                current_at: requested_at,
                clock: Arc::clone(&self.updater.clock),
                max_entry_bytes: self.updater.limits.max_entry_bytes,
                progress: Arc::clone(&self.progress),
                cancel: Arc::clone(&self.cancel),
                deadline,
            },
            update_lock: Arc::clone(&self.update_lock),
        };
        let installed = tokio::task::spawn_blocking(move || install::install(job)).await;
        facts.install_ms = elapsed_ms(began);
        let records = match installed {
            Ok(result) => result?,
            Err(join_error) => {
                return Err(Stop::Failed(UpdateFailure::Store(cleaned(&format!(
                    "the install stopped: {join_error}"
                )))));
            }
        };
        // The commit fixed the outcome; nothing after it can change it.
        (self.progress)(UpdateProgress::Ending);
        facts.records = records;
        self.checkpoint().await;
        Ok(UpdateOutcome::Installed {
            records,
            archive_bytes,
        })
    }

    /// Best effort, after an install: folds the WAL back into the store on a blocking thread
    /// that holds the update lock too.
    async fn checkpoint(&self) {
        let db = self.files.db.clone();
        let update_lock = Arc::clone(&self.update_lock);
        let ran = tokio::task::spawn_blocking(move || {
            install::checkpoint(&db);
            drop(update_lock);
        })
        .await;
        if let Err(error) = ran {
            debug!(error = %cleaned(&error.to_string()), "the checkpoint after the install stopped");
        }
    }

    /// Step 7: a store to install into. An unreadable store is deleted and a new one created
    /// only now that the download is complete, under the store's exclusive lock, which is
    /// released only once the new file exists and is marked as being rebuilt, so a check never
    /// finds the old store gone and no new one made, and a rebuild that fails after the mark
    /// leaves a store that still reads Unreadable. A missing store is created the same way,
    /// unmarked.
    fn prepare_store(&self, rebuild: bool, now: DateTime<Utc>) -> Result<(), Stop> {
        let exists = match std::fs::symlink_metadata(&self.files.db) {
            Ok(_) => true,
            Err(error) if store::is_absent(&error) => false,
            Err(error) => return Err(Stop::Failed(UpdateFailure::disk(&error))),
        };
        if exists && !rebuild {
            return Ok(());
        }
        let _store = match locks::try_exclusive(&self.files.lock) {
            Ok(lock) => lock,
            Err(LockFailure::Held) => return Err(Stop::Failed(UpdateFailure::InUse)),
            Err(LockFailure::Failed(reason)) => {
                return Err(Stop::Failed(UpdateFailure::Lock(reason)));
            }
        };
        if rebuild {
            let bytes = store::delete_store(&self.files).map_err(|e| UpdateFailure::disk(&e))?;
            info!(
                ecosystem = self.ecosystem.name(),
                bytes, "offline data deleted to be rebuilt"
            );
        }
        store::create(&self.files).map_err(|e| UpdateFailure::disk(&e))?;
        let mut conn = store::connection(&self.files.db).map_err(|e| UpdateFailure::store(&e))?;
        // The mark goes first, so a folder that refuses WAL mode still leaves a store that reads
        // as being rebuilt.
        if rebuild {
            store::mark_rebuilding(&mut conn, now).map_err(|e| UpdateFailure::store(&e))?;
        }
        match store::set_wal(&conn) {
            Ok(true) => Ok(()),
            Ok(false) => Err(Stop::Failed(wal_refused())),
            Err(error) => Err(Stop::Failed(UpdateFailure::store(&error))),
        }
    }

    /// Best effort: a leftover `.part` is removed by the next update before it writes one.
    fn remove_part(&self) {
        if let Err(error) = store::remove_if_present(&self.files.part) {
            debug!(error = %cleaned(&error.to_string()), "the download file was not deleted");
        }
    }
}

/// `<base><osv name>/all.zip`. Joining fails only for a base that cannot hold a path (a
/// `data:` address), which only a caller of [`Updater::with_parts`] can pass; the request then
/// cannot be sent.
fn archive_url(base: &Url, ecosystem: Ecosystem) -> Result<Url, UpdateFailure> {
    base.join(&format!("{}/all.zip", encode_segment(ecosystem.osv_name())))
        .map_err(|_| {
            UpdateFailure::Request(SourceFailure::Unreachable {
                detail: "the download address could not be built".to_owned(),
            })
        })
}

/// The failure for a folder where SQLite cannot use WAL mode.
fn wal_refused() -> UpdateFailure {
    UpdateFailure::Store(
        "SQLite cannot use WAL mode in this folder (is the home folder on a network share?)"
            .to_owned(),
    )
}

/// The word for the status a run found the data in.
fn status_word(status: &DataStatus) -> &'static str {
    match status {
        DataStatus::NotDownloaded => "not_downloaded",
        DataStatus::Ready { .. } => "ready",
        DataStatus::Unavailable { .. } => "unavailable",
        DataStatus::Locked => "locked",
        DataStatus::Unreadable { .. } => "unreadable",
        DataStatus::NewerFormat => "newer_format",
    }
}

/// The update's one finished line: info for an outcome that is not a fault (installed,
/// unchanged, busy, skipped, in use, cancelled), warn for every other failure.
fn log_finished(ecosystem: Ecosystem, trigger: Trigger, outcome: &UpdateOutcome, facts: &Facts) {
    let (word, failure, skip) = match outcome {
        UpdateOutcome::Installed { .. } => ("installed", None, None),
        UpdateOutcome::Unchanged => ("unchanged", None, None),
        UpdateOutcome::Busy => ("busy", None, None),
        UpdateOutcome::Skipped(skip) => ("skipped", None, Some(*skip)),
        UpdateOutcome::Cancelled { .. } => ("cancelled", None, None),
        UpdateOutcome::Failed(failure) => ("failed", Some(failure), None),
    };
    let kind = failure.map_or("", UpdateFailure::kind);
    let reason = match (failure, skip) {
        (Some(failure), _) => failure.describe(ecosystem),
        (None, Some(skip)) => skip.name().to_owned(),
        (None, None) => String::new(),
    };
    let reason = reason.as_str();
    let from = facts.from;
    let fault = failure.is_some_and(|failure| *failure != UpdateFailure::InUse);
    if fault {
        warn!(
            ecosystem = ecosystem.name(),
            trigger = trigger.name(),
            from,
            outcome = word,
            kind,
            archive_bytes = facts.archive_bytes,
            received = facts.received,
            records = facts.records,
            download_ms = facts.download_ms,
            install_ms = facts.install_ms,
            reason,
            attempt_write_failed = facts.attempt_write_failed,
            "offline update finished"
        );
    } else {
        info!(
            ecosystem = ecosystem.name(),
            trigger = trigger.name(),
            from,
            outcome = word,
            kind,
            archive_bytes = facts.archive_bytes,
            received = facts.received,
            records = facts.records,
            download_ms = facts.download_ms,
            install_ms = facts.install_ms,
            reason,
            attempt_write_failed = facts.attempt_write_failed,
            "offline update finished"
        );
    }
}

#[cfg(test)]
mod tests;
