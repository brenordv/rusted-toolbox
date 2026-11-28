//! The archive's writer thread. It owns every capture-side write: it takes
//! whatever requests are queued, applies them in one write transaction,
//! commits once at redb's default `Durability::Immediate`, and only then
//! answers each request. Async callers never touch redb or wait on an fsync on
//! a runtime thread.

use crate::store::schema::{
    BY_TIME, CHECKPOINTS, CREATED_ON_KEY, EVENT_HUB_KEY, MESSAGES, META, NAMESPACE_KEY,
    NO_ENQUEUED_TIME, decode_message, encode_checkpoint, encode_created_on, encode_message,
    message_table_key, time_key,
};
use anyhow::{Context, anyhow};
use chrono::{DateTime, TimeDelta, Utc};
use eh_core::connection::EventHubIdentity;
use eh_core::engine::{Checkpoint, StoreOutcome};
use eh_core::models::CapturedMessage;
use redb::{Database, ReadableTable, Table, WriteTransaction};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use tracing::{debug, info, trace, warn};

/// Most requests applied in one transaction.
const MAX_BATCH: usize = 256;
/// A batch slower than this (waiting for the write lock plus writing and
/// committing) is reported.
const SLOW_BATCH: Duration = Duration::from_secs(2);
/// At most one slow-batch warning per this interval.
const SLOW_BATCH_WARNING_INTERVAL: Duration = Duration::from_secs(60);
/// How much earlier than the event hub's creation time an archived event may be enqueued
/// before it counts as an older hub's: the creation time and the enqueue times come from
/// different service clocks.
const CREATION_CLOCK_TOLERANCE: TimeDelta = TimeDelta::minutes(1);

type Reply<T> = oneshot::Sender<anyhow::Result<T>>;

/// A write the writer thread applies. `outcome` starts as an error and is
/// replaced when the request is applied, so a request is never answered with
/// a success it did not get.
pub(crate) enum WriteRequest {
    Bind {
        hub: EventHubIdentity,
        outcome: anyhow::Result<()>,
        reply: Reply<()>,
    },
    Store {
        message: Box<CapturedMessage>,
        checkpoint: Checkpoint,
        dedup: bool,
        outcome: anyhow::Result<StoreOutcome>,
        reply: Reply<StoreOutcome>,
    },
}

impl WriteRequest {
    pub(crate) fn bind(hub: EventHubIdentity) -> (Self, oneshot::Receiver<anyhow::Result<()>>) {
        let (reply, receiver) = oneshot::channel();
        let request = Self::Bind {
            hub,
            outcome: Err(not_applied()),
            reply,
        };
        (request, receiver)
    }

    pub(crate) fn store(
        message: CapturedMessage,
        checkpoint: Checkpoint,
        dedup: bool,
    ) -> (Self, oneshot::Receiver<anyhow::Result<StoreOutcome>>) {
        let (reply, receiver) = oneshot::channel();
        let request = Self::Store {
            message: Box::new(message),
            checkpoint,
            dedup,
            outcome: Err(not_applied()),
            reply,
        };
        (request, receiver)
    }

    /// Answers the request: with `failure` when the batch failed, otherwise
    /// with its own outcome.
    fn settle(self, failure: Option<&str>) {
        match self {
            Self::Bind { outcome, reply, .. } => answer(reply, settled(outcome, failure)),
            Self::Store { outcome, reply, .. } => answer(reply, settled(outcome, failure)),
        }
    }
}

fn not_applied() -> anyhow::Error {
    anyhow!("The archive writer did not apply this request")
}

fn settled<T>(outcome: anyhow::Result<T>, failure: Option<&str>) -> anyhow::Result<T> {
    match failure {
        Some(cause) => Err(anyhow!(cause.to_owned())),
        None => outcome,
    }
}

fn answer<T>(reply: Reply<T>, result: anyhow::Result<T>) {
    if reply.send(result).is_err() {
        // Best-effort: the caller dropped its future (it stopped waiting, as
        // when a run future is dropped); the write itself is already settled.
        debug!("An archive write finished after its caller stopped waiting");
    }
}

enum WriterMessage {
    Request(WriteRequest),
    Shutdown,
}

/// The running writer thread. Dropping it asks the thread to finish the
/// requests already queued and waits for it to exit.
pub(crate) struct WriterHandle {
    sender: Sender<WriterMessage>,
    thread: Option<JoinHandle<()>>,
    /// Why the writer thread died, once it has.
    stopped: Arc<OnceLock<String>>,
    /// Why a batch failed, once one has. Only the writer thread sets it, before it answers the
    /// failed batch.
    failed: Arc<OnceLock<String>>,
    path: PathBuf,
}

/// Applies one batch of requests to the database.
type ApplyBatch = fn(&Database, &mut [WriteRequest]) -> anyhow::Result<BatchTiming>;

impl WriterHandle {
    /// Starts the writer thread for the archive at `path`.
    pub(crate) fn spawn(database: Arc<Database>, path: PathBuf) -> anyhow::Result<Self> {
        Self::spawn_with(database, path, apply_batch)
    }

    fn spawn_with(
        database: Arc<Database>,
        path: PathBuf,
        apply: ApplyBatch,
    ) -> anyhow::Result<Self> {
        let (sender, receiver) = channel();
        let stopped = Arc::new(OnceLock::new());
        let thread_stopped = Arc::clone(&stopped);
        let failed = Arc::new(OnceLock::new());
        let thread_failed = Arc::clone(&failed);
        let thread_path = path.clone();
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        let thread = std::thread::Builder::new()
            .name("eh-archive-writer".to_owned())
            .spawn(move || {
                tracing::dispatcher::with_default(&dispatch, || {
                    // The database is never touched again after a panic: the
                    // thread records the cause and exits.
                    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                        let causes = FailureCauses {
                            stopped: &thread_stopped,
                            failed: &thread_failed,
                        };
                        run_writer(&database, &receiver, &thread_path, causes, apply);
                    }));
                    if let Err(panic) = result {
                        record_panic(&thread_stopped, &thread_path, panic.as_ref());
                    }
                });
            })
            .with_context(|| {
                format!(
                    "Failed to start the writer thread for the archive '{}'",
                    path.display()
                )
            })?;

        Ok(Self {
            sender,
            thread: Some(thread),
            stopped,
            failed,
            path,
        })
    }

    /// Why this writer applies no more requests: the cause of its failed batch, or of its
    /// death. `None` while it still writes.
    pub(crate) fn write_failure(&self) -> Option<String> {
        self.failed.get().or_else(|| self.stopped.get()).cloned()
    }

    /// Queues `request`. Fails when the writer has stopped.
    pub(crate) fn submit(&self, request: WriteRequest) -> anyhow::Result<()> {
        self.sender
            .send(WriterMessage::Request(request))
            .map_err(|_| self.stopped_error())
    }

    /// The error a caller gets when the writer is gone.
    pub(crate) fn stopped_error(&self) -> anyhow::Error {
        let cause = self
            .stopped
            .get()
            .map_or("it exited unexpectedly", String::as_str);
        anyhow!(
            "The archive writer for '{}' stopped: {cause}",
            self.path.display()
        )
    }

    /// Asks the writer to finish the queued requests, waits for it to exit,
    /// and reports a writer that died instead.
    pub(crate) fn stop(mut self) -> anyhow::Result<()> {
        self.shut_down();
        match self.stopped.get() {
            Some(_) => Err(self.stopped_error()),
            None => Ok(()),
        }
    }

    fn shut_down(&mut self) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        if self.sender.send(WriterMessage::Shutdown).is_err() {
            // Best-effort: the writer already exited, so there is nothing to
            // tell it; joining below still collects it.
            debug!(path = %self.path.display(), "The archive writer had already exited");
        }
        if thread.join().is_err() {
            // Best-effort: the writer's own panic handler already recorded and
            // logged the cause; this only notes that the join saw it.
            warn!(path = %self.path.display(), "The archive writer thread ended abnormally");
        }
    }
}

impl Drop for WriterHandle {
    fn drop(&mut self) {
        self.shut_down();
    }
}

/// Logs a writer panic and keeps its cause for the errors callers get.
/// Returns the cause.
fn record_panic(
    stopped: &OnceLock<String>,
    path: &Path,
    panic: &(dyn std::any::Any + Send),
) -> String {
    let cause = panic_message(panic);
    warn!(
        path = %path.display(),
        cause = %cause,
        "The archive writer stopped unexpectedly"
    );
    stopped.get_or_init(|| cause.clone());
    cause
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = panic.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = panic.downcast_ref::<String>() {
        text.clone()
    } else {
        "a panic without a message".to_owned()
    }
}

/// Per-run writer statistics, logged once when the writer exits.
#[derive(Default)]
struct WriterCounters {
    items: u64,
    batches: u64,
    max_batch: usize,
    lock_wait: Duration,
    commit_total: Duration,
    commit_max: Duration,
    last_slow_warning: Option<Instant>,
}

impl WriterCounters {
    fn record(&mut self, path: &Path, items: usize, timing: BatchTiming) {
        self.items += items as u64;
        self.batches += 1;
        self.max_batch = self.max_batch.max(items);
        self.lock_wait += timing.lock_wait;
        self.commit_total += timing.write;
        self.commit_max = self.commit_max.max(timing.write);
        trace!(
            items,
            elapsed_ms = timing.write.as_millis(),
            "Committed an archive batch"
        );

        let elapsed = timing.lock_wait + timing.write;
        let warning_due = self
            .last_slow_warning
            .is_none_or(|last| last.elapsed() >= SLOW_BATCH_WARNING_INTERVAL);
        if elapsed >= SLOW_BATCH && warning_due {
            warn!(
                path = %path.display(),
                elapsed_ms = elapsed.as_millis(),
                lock_wait_ms = timing.lock_wait.as_millis(),
                "An archive write was slow; is the disk busy or the file being scanned?"
            );
            self.last_slow_warning = Some(Instant::now());
        }
    }

    fn log_summary(&self, path: &Path) {
        info!(
            path = %path.display(),
            items = self.items,
            batches = self.batches,
            max_batch = self.max_batch,
            lock_wait_ms = self.lock_wait.as_millis(),
            commit_total_ms = self.commit_total.as_millis(),
            commit_max_ms = self.commit_max.as_millis(),
            "The archive writer stopped"
        );
    }
}

#[derive(Clone, Copy)]
pub(super) struct BatchTiming {
    lock_wait: Duration,
    write: Duration,
}

/// The cells the writer thread records its failures in.
#[derive(Clone, Copy)]
struct FailureCauses<'a> {
    stopped: &'a OnceLock<String>,
    failed: &'a OnceLock<String>,
}

/// The writer loop: block for one request, take whatever else is queued,
/// apply them together, answer, repeat until shut down. A panic while
/// applying a batch answers that batch with its cause and ends the loop. A
/// batch that fails is answered with its cause, and so is every later one;
/// each cause is recorded before the batch is answered.
fn run_writer(
    database: &Database,
    receiver: &Receiver<WriterMessage>,
    path: &Path,
    causes: FailureCauses<'_>,
    apply: ApplyBatch,
) {
    let mut counters = WriterCounters::default();
    let mut shutting_down = false;

    while !shutting_down {
        let Ok(first) = receiver.recv() else {
            break;
        };
        let mut batch = Vec::new();
        let mut next = Some(first);
        while let Some(message) = next {
            match message {
                WriterMessage::Request(request) => batch.push(request),
                WriterMessage::Shutdown => shutting_down = true,
            }
            next = if shutting_down || batch.len() >= MAX_BATCH {
                None
            } else {
                receiver.try_recv().ok()
            };
        }
        if batch.is_empty() {
            continue;
        }

        if let Some(cause) = causes.failed.get() {
            let message = format!(
                "Writing to the archive '{}' failed earlier: {cause}",
                path.display()
            );
            settle_all(batch, Some(&message));
            continue;
        }

        let items = batch.len();
        let applied = std::panic::catch_unwind(AssertUnwindSafe(|| apply(database, &mut batch)));
        match applied {
            Ok(Ok(timing)) => {
                counters.record(path, items, timing);
                settle_all(batch, None);
            }
            Err(panic) => {
                let cause = record_panic(causes.stopped, path, panic.as_ref());
                let message = format!(
                    "The archive writer for '{}' stopped: {cause}",
                    path.display()
                );
                settle_all(batch, Some(&message));
                break;
            }
            Ok(Err(error)) => {
                let cause = format!("{error:#}");
                warn!(path = %path.display(), error = %cause, "Writing to the archive failed");
                let message = format!(
                    "Writing to the archive '{}' failed: {cause}",
                    path.display()
                );
                causes.failed.get_or_init(|| cause);
                settle_all(batch, Some(&message));
            }
        }
    }

    counters.log_summary(path);
}

fn settle_all(batch: Vec<WriteRequest>, failure: Option<&str>) {
    for request in batch {
        request.settle(failure);
    }
}

/// Applies every request in one transaction and commits it. A request the
/// archive refuses (another hub, a different event at the same key) gets its
/// own error and writes nothing; a storage failure fails the whole batch.
pub(super) fn apply_batch(
    database: &Database,
    batch: &mut [WriteRequest],
) -> anyhow::Result<BatchTiming> {
    let started = Instant::now();
    let transaction = database
        .begin_write()
        .context("Failed to start an archive write transaction")?;
    let lock_wait = started.elapsed();

    {
        let mut tables = WriteTables::open(&transaction)?;
        let mut binding = tables.binding()?;
        for request in batch.iter_mut() {
            match request {
                WriteRequest::Bind { hub, outcome, .. } => {
                    *outcome = tables.bind(&mut binding, hub)?;
                }
                WriteRequest::Store {
                    message,
                    checkpoint,
                    dedup,
                    outcome,
                    ..
                } => {
                    *outcome = tables.store(binding.is_some(), message, checkpoint, *dedup)?;
                }
            }
        }
    }
    transaction
        .commit()
        .context("Failed to commit an archive write transaction")?;

    Ok(BatchTiming {
        lock_wait,
        write: started.elapsed() - lock_wait,
    })
}

/// The tables a capture batch writes. Each method returns `Err` for a storage
/// failure (the batch fails) and `Ok(Err(..))` for a refused request (nothing
/// written for it).
struct WriteTables<'txn> {
    meta: Table<'txn, &'static str, &'static str>,
    messages: Table<'txn, (&'static str, i64), &'static [u8]>,
    by_time: Table<'txn, (i64, &'static str, i64), ()>,
    checkpoints: Table<'txn, &'static str, &'static [u8]>,
}

impl<'txn> WriteTables<'txn> {
    fn open(transaction: &'txn WriteTransaction) -> anyhow::Result<Self> {
        Ok(Self {
            meta: transaction
                .open_table(META)
                .context("Failed to open the archive's meta table")?,
            messages: transaction
                .open_table(MESSAGES)
                .context("Failed to open the archive's messages table")?,
            by_time: transaction
                .open_table(BY_TIME)
                .context("Failed to open the archive's time index")?,
            checkpoints: transaction
                .open_table(CHECKPOINTS)
                .context("Failed to open the archive's checkpoints table")?,
        })
    }

    fn binding(&self) -> anyhow::Result<Option<EventHubIdentity>> {
        crate::store::read_binding(&self.meta)
    }

    fn bind(
        &mut self,
        binding: &mut Option<EventHubIdentity>,
        hub: &EventHubIdentity,
    ) -> anyhow::Result<anyhow::Result<()>> {
        match binding {
            Some(bound) if same_event_hub(bound, hub) => self.rebind(bound, hub),
            Some(bound) => Ok(Err(anyhow!(
                "This data folder's archive belongs to '{}/{}'; the run targets '{}/{}'. Use \
                 another data folder.",
                bound.fully_qualified_namespace,
                bound.event_hub_name,
                hub.fully_qualified_namespace,
                hub.event_hub_name
            ))),
            None => {
                self.meta
                    .insert(NAMESPACE_KEY, hub.fully_qualified_namespace.as_str())
                    .context("Failed to record the archive's namespace")?;
                self.meta
                    .insert(EVENT_HUB_KEY, hub.event_hub_name.as_str())
                    .context("Failed to record the archive's event hub")?;
                if let Some(created_on) = hub.created_on {
                    self.record_created_on(created_on)?;
                }
                info!(
                    namespace = %hub.fully_qualified_namespace,
                    event_hub = %hub.event_hub_name,
                    created_on = %describe_created_on(hub.created_on),
                    "Bound the archive to its event hub"
                );
                *binding = Some(hub.clone());
                Ok(Ok(()))
            }
        }
    }

    /// Binds again to the event hub `bound` names, checking that it is the same hub: one
    /// created at another time, or one created after the archive's earliest event, is a
    /// deleted and recreated hub. An archive bound without a creation time records the
    /// reported one, in `meta` and in `bound`, the batch's view of the binding.
    fn rebind(
        &mut self,
        bound: &mut EventHubIdentity,
        hub: &EventHubIdentity,
    ) -> anyhow::Result<anyhow::Result<()>> {
        let Some(reported) = hub.created_on else {
            if bound.created_on.is_some() {
                warn!(
                    namespace = %hub.fully_qualified_namespace,
                    event_hub = %hub.event_hub_name,
                    "The service did not report the event hub's creation time; a recreated \
                     event hub cannot be detected on this run"
                );
            }
            return Ok(Ok(()));
        };
        if let Some(archived) = bound.created_on
            && archived != reported
        {
            warn!(
                namespace = %hub.fully_qualified_namespace,
                event_hub = %hub.event_hub_name,
                archived_created_on = %encode_created_on(archived),
                reported_created_on = %encode_created_on(reported),
                "The event hub's creation time differs from the archive's"
            );
            return Ok(Err(anyhow!(
                "The event hub '{}/{}' was created on {}; this data folder's archive holds the \
                 one created on {}: the event hub was deleted and recreated (or replaced). Use \
                 another data folder.",
                hub.fully_qualified_namespace,
                hub.event_hub_name,
                display_time(reported),
                display_time(archived)
            )));
        }
        if let Some(earliest) = self.earliest_enqueued_time()?
            && reported.signed_duration_since(earliest) > CREATION_CLOCK_TOLERANCE
        {
            warn!(
                namespace = %hub.fully_qualified_namespace,
                event_hub = %hub.event_hub_name,
                earliest_enqueued = %encode_created_on(earliest),
                reported_created_on = %encode_created_on(reported),
                "The archive holds events older than the event hub"
            );
            return Ok(Err(anyhow!(
                "This data folder's archive holds events enqueued on {}, before the event hub \
                 '{}/{}' was created on {}: the event hub was deleted and recreated (or \
                 replaced). Use another data folder.",
                display_time(earliest),
                hub.fully_qualified_namespace,
                hub.event_hub_name,
                display_time(reported)
            )));
        }
        if bound.created_on.is_none() {
            self.record_created_on(reported)?;
            bound.created_on = Some(reported);
            info!(
                namespace = %hub.fully_qualified_namespace,
                event_hub = %hub.event_hub_name,
                created_on = %encode_created_on(reported),
                "Recorded the event hub's creation time in the archive"
            );
        }
        trace!(
            namespace = %hub.fully_qualified_namespace,
            event_hub = %hub.event_hub_name,
            "The archive is already bound to this event hub"
        );
        Ok(Ok(()))
    }

    fn record_created_on(&mut self, created_on: DateTime<Utc>) -> anyhow::Result<()> {
        self.meta
            .insert(CREATED_ON_KEY, encode_created_on(created_on).as_str())
            .context("Failed to record the event hub's creation time")?;
        Ok(())
    }

    /// The earliest enqueue time among the archived events, from the time index; events
    /// without an enqueue time sort first under their own key and are skipped.
    fn earliest_enqueued_time(&self) -> anyhow::Result<Option<DateTime<Utc>>> {
        let first = self
            .by_time
            .range((NO_ENQUEUED_TIME + 1, "", i64::MIN)..)
            .context("Failed to read the archive's time index")?
            .next()
            .transpose()
            .context("Failed to read the archive's time index")?;
        Ok(first.and_then(|(key, _)| DateTime::from_timestamp_millis(key.value().0)))
    }

    fn store(
        &mut self,
        bound: bool,
        message: &CapturedMessage,
        checkpoint: &Checkpoint,
        dedup: bool,
    ) -> anyhow::Result<anyhow::Result<StoreOutcome>> {
        if !bound {
            return Ok(Err(anyhow!(
                "The archive is not bound to an event hub; bind it before storing messages"
            )));
        }
        let key = message_table_key(&message.key);
        let existing = match self.existing_message(message)? {
            Ok(existing) => existing,
            Err(refused) => return Ok(Err(refused)),
        };
        let encoded = encode_message(message)?;
        let encoded_checkpoint = encode_checkpoint(checkpoint)?;

        if let Some(stored) = &existing {
            // A sequence number names one event only while the hub exists: a
            // recreated hub starts over, so the same key with another enqueue
            // time is a different event, never a duplicate or an update.
            if let (Some(stored_time), Some(new_time)) =
                (stored.enqueued_time, message.enqueued_time)
                && stored_time != new_time
            {
                return Ok(Err(anyhow!(
                    "The archive already holds a different event at partition {}, sequence {}; \
                     the event hub was likely deleted and recreated. Use another data folder.",
                    message.key.partition_id,
                    message.key.sequence_number
                )));
            }
            if dedup {
                return Ok(Ok(StoreOutcome {
                    newly_stored: false,
                }));
            }
            let stored_time_key = time_key(stored);
            if stored_time_key != time_key(message) {
                self.by_time
                    .remove((stored_time_key, key.0, key.1))
                    .context("Failed to remove a stale time-index entry")?;
            }
        }

        self.messages
            .insert(key, encoded.as_slice())
            .context("Failed to insert a message")?;
        self.by_time
            .insert((time_key(message), key.0, key.1), ())
            .context("Failed to insert a time-index entry")?;
        self.checkpoints
            .insert(
                checkpoint.partition_id.as_str(),
                encoded_checkpoint.as_slice(),
            )
            .context("Failed to update a checkpoint")?;

        Ok(Ok(StoreOutcome { newly_stored: true }))
    }

    /// The message already stored under `message`'s key; a stored value that
    /// does not decode is a refusal (nothing is overwritten).
    fn existing_message(
        &self,
        message: &CapturedMessage,
    ) -> anyhow::Result<anyhow::Result<Option<CapturedMessage>>> {
        let key = message_table_key(&message.key);
        let stored = self
            .messages
            .get(key)
            .context("Failed to read a stored message")?;
        Ok(stored
            .map(|guard| decode_message(key.0, key.1, guard.value()))
            .transpose())
    }
}

/// A time as the archive's refusal messages show it: `2026-11-14 21:30:05.123 UTC`.
fn display_time(time: DateTime<Utc>) -> String {
    time.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string()
}

/// A creation time for the log in its stored form, or "not reported".
fn describe_created_on(created_on: Option<DateTime<Utc>>) -> String {
    created_on.map_or_else(|| "not reported".to_owned(), encode_created_on)
}

/// Event hub names compare case-insensitively; the namespace arrives
/// lowercased from validation, so it is compared the same way.
fn same_event_hub(left: &EventHubIdentity, right: &EventHubIdentity) -> bool {
    left.fully_qualified_namespace
        .eq_ignore_ascii_case(&right.fully_qualified_namespace)
        && left
            .event_hub_name
            .eq_ignore_ascii_case(&right.event_hub_name)
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) mod test_writers {
    //! Writer replacements that make the writer-death path reachable.

    use super::*;

    /// Starts a writer that panics with "simulated writer bug" when it
    /// applies its first batch.
    pub(crate) fn spawn_panicking(
        database: Arc<Database>,
        path: PathBuf,
    ) -> anyhow::Result<WriterHandle> {
        WriterHandle::spawn_with(database, path, |_, _| panic!("simulated writer bug"))
    }
}
