//! The capture engine: connects, runs one task per partition, stores every
//! event through a [`MessageSink`], and streams view events and statistics
//! to its consumer.

mod partition;
mod start;

use crate::connection::{
    EventHubConnection, EventHubIdentity, EventHubTarget, close_client_quietly, connect,
};
use crate::filters::CaptureFilters;
use crate::models::CapturedMessage;
use crate::stats::{StatsSnapshot, StatsTracker};
use anyhow::Context;
use azure_messaging_eventhubs::ConsumerClient;
use chrono::{DateTime, Utc};
use partition::{PartitionContext, run_partition};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt::{Display, Formatter};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::task::{Id, JoinSet};
use tokio::time::MissedTickBehavior;
use tracing::{info, warn};

/// Capacity of the engine-to-consumer event channel.
pub const EVENT_CHANNEL_CAPACITY: usize = 1024;
const COMMAND_CHANNEL_CAPACITY: usize = 16;
/// Lower limit for [`EngineConfig::feedback_interval`]; a zero interval
/// would spin the statistics publisher.
const MIN_FEEDBACK_INTERVAL: Duration = Duration::from_millis(100);

/// Requests a consumer sends to a running engine.
#[derive(Clone, Debug)]
pub enum EngineCommand {
    /// Replaces the capture filters. Applies to events processed after the
    /// update; already-emitted events stay as they are.
    UpdateFilters(CaptureFilters),
    /// Stops every partition gracefully; the run then ends with
    /// [`RunEnd::Stopped`].
    Stop,
}

/// Why an event was not emitted as [`EngineEvent::Message`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// Stored, but the capture filters did not pass it.
    Filtered,
    /// Already processed: a re-delivery after a reconnect, or already in the
    /// sink.
    Duplicate,
    /// No sequence number, so it could not be keyed or stored.
    Unkeyable,
    /// Stored, but a time filter is active and the event has no enqueue
    /// time.
    NoEnqueuedTime,
}

/// How a partition's capture ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartitionEnd {
    /// An event enqueued after the time window's upper bound arrived.
    UpperBoundPassed,
    /// The time window was closed and every event in it has been read.
    NothingLeftInWindow,
    /// A stop was requested.
    Stopped,
    Failed(String),
}

impl Display for PartitionEnd {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UpperBoundPassed => f.write_str("passed the end of the time window"),
            Self::NothingLeftInWindow => f.write_str("read everything in the time window"),
            Self::Stopped => f.write_str("stopped on request"),
            Self::Failed(reason) => write!(f, "failed: {reason}"),
        }
    }
}

/// What the engine reports to its consumer.
///
/// `Message` and `Skipped` are view events: when the consumer lags they are
/// dropped and counted in [`StatsSnapshot::ui_dropped`], because the sink, not
/// the view, is the archive. The others are control events and always
/// arrive (the engine waits for channel space).
#[derive(Clone, Debug)]
pub enum EngineEvent {
    /// Connected; one task per listed partition starts next.
    Started { partitions: Vec<String> },
    /// A stored event that passed the capture filters.
    Message(Box<CapturedMessage>),
    Skipped {
        partition_id: String,
        reason: SkipReason,
    },
    PartitionEnded {
        partition_id: String,
        end: PartitionEnd,
    },
    /// A failure. `fatal` is set when the run cannot start (connection or
    /// partition selection), in which case the run future also returns the
    /// error; otherwise only `partition_id`'s task ended.
    Error {
        partition_id: Option<String>,
        error: String,
        fatal: bool,
    },
}

/// The position up to which a partition has been durably stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub partition_id: String,
    pub sequence_number: i64,
    pub offset: Option<String>,
    /// Broker enqueue time of the checkpointed event.
    pub enqueued_time: Option<DateTime<Utc>>,
    /// Local time the checkpoint was written.
    pub updated_at: DateTime<Utc>,
}

/// Result of [`MessageSink::store`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoreOutcome {
    /// `false` when the sink already held the message (dedup on).
    pub newly_stored: bool,
}

/// Durable storage for captured events and their checkpoints.
pub trait MessageSink: Send + Sync + 'static {
    /// Ties the sink to the event hub a run reads, before any checkpoint is
    /// loaded. Must succeed again for the same hub, and must fail when the
    /// sink already holds another event hub's data.
    fn bind(&self, hub: &EventHubIdentity) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Stores `message` and `checkpoint` as one durable unit. With `dedup`
    /// on, a message whose key is already stored is left alone and reported
    /// as not newly stored; with it off, the message is written again.
    ///
    /// Must be cancel-safe: dropping the returned future either completes or
    /// discards the unit, never half of it. A graceful stop never cancels a
    /// store, but dropping the engine's run future does.
    fn store(
        &self,
        message: &CapturedMessage,
        checkpoint: &Checkpoint,
        dedup: bool,
    ) -> impl Future<Output = anyhow::Result<StoreOutcome>> + Send;

    /// Returns the stored checkpoint of `partition_id`, or `None` when it has
    /// none.
    fn load_checkpoint(
        &self,
        partition_id: &str,
    ) -> impl Future<Output = anyhow::Result<Option<Checkpoint>>> + Send;
}

/// Settings for one capture run.
#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub target: EventHubTarget,
    /// Partitions to read; `None` reads all of them.
    pub partitions: Option<Vec<String>>,
    pub initial_filters: CaptureFilters,
    /// Start from the filter bound or the earliest event instead of the
    /// stored checkpoint, and store every event again.
    pub ignore_checkpoint: bool,
    /// Receiver prefetch credits; `None` keeps the SDK default.
    pub prefetch: Option<u32>,
    /// How often a statistics snapshot is published (at least 100 ms).
    pub feedback_interval: Duration,
}

/// The end state of a run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunEnd {
    /// Every partition ended on its own and none failed.
    Completed,
    /// A stop was requested; [`RunSummary::partitions`] still shows any
    /// partition that failed before it.
    Stopped,
    /// At least one partition failed; the text names them.
    Failed(String),
}

/// What a finished run reports.
#[derive(Clone, Debug)]
pub struct RunSummary {
    pub stats: StatsSnapshot,
    pub partitions: BTreeMap<String, PartitionEnd>,
    pub end: RunEnd,
}

/// The consumer's side of a run.
///
/// Drain `events` independently of sending `commands`: control events wait
/// for channel space, so a consumer that blocks on a full command channel
/// while not reading events stalls the run.
#[derive(Debug)]
pub struct EngineChannels {
    pub commands: mpsc::Sender<EngineCommand>,
    pub events: mpsc::Receiver<EngineEvent>,
    /// The latest statistics; never queues stale values.
    pub stats: watch::Receiver<StatsSnapshot>,
}

/// Entry point for capture runs.
pub struct CaptureEngine;

impl CaptureEngine {
    /// Prepares a run and returns its channels and the run future. Nothing
    /// happens until the future is polled; it resolves once a stop is
    /// requested or no partition task remains, and returns an error only
    /// when the run cannot start. Send [`EngineCommand::Stop`] for a graceful
    /// end; dropping the future aborts the partition tasks mid-flight.
    pub fn build<S: MessageSink>(
        config: EngineConfig,
        sink: S,
    ) -> (
        EngineChannels,
        impl Future<Output = anyhow::Result<RunSummary>> + Send + 'static,
    ) {
        let (commands_tx, commands_rx) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);
        let (events_tx, events_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let stats = Arc::new(StatsTracker::new(Utc::now()));
        let (stats_tx, stats_rx) = watch::channel(stats.snapshot());
        let channels = EngineChannels {
            commands: commands_tx,
            events: events_rx,
            stats: stats_rx,
        };

        let run = run(
            config,
            Arc::new(sink),
            commands_rx,
            events_tx,
            stats,
            stats_tx,
        );
        (channels, run)
    }
}

async fn run<S: MessageSink>(
    config: EngineConfig,
    sink: Arc<S>,
    commands: mpsc::Receiver<EngineCommand>,
    events: mpsc::Sender<EngineEvent>,
    stats: Arc<StatsTracker>,
    stats_tx: watch::Sender<StatsSnapshot>,
) -> anyhow::Result<RunSummary> {
    let (connection, partitions) = match start_run(&config, sink.as_ref()).await {
        Ok(started) => started,
        Err(error) => {
            send_control(
                &events,
                EngineEvent::Error {
                    partition_id: None,
                    error: format!("{error:#}"),
                    fatal: true,
                },
            )
            .await;
            return Err(error);
        }
    };
    send_control(
        &events,
        EngineEvent::Started {
            partitions: partitions.clone(),
        },
    )
    .await;

    let client = Arc::new(connection.client);
    let (filters_tx, _) = watch::channel(config.initial_filters);
    let (shutdown_tx, _) = watch::channel(false);
    let mut tasks = JoinSet::new();
    let mut partition_by_task = HashMap::new();
    for partition_id in partitions {
        let context = PartitionContext {
            partition_id: partition_id.clone(),
            sink: Arc::clone(&sink),
            events: events.clone(),
            stats: Arc::clone(&stats),
            filters: filters_tx.subscribe(),
            shutdown: shutdown_tx.subscribe(),
            ignore_checkpoint: config.ignore_checkpoint,
            prefetch: config.prefetch,
            checkpoint_floor: None,
        };
        let handle = tasks.spawn(run_partition(Arc::clone(&client), context));
        partition_by_task.insert(handle.id(), partition_id);
    }

    let supervisor = Supervisor {
        commands,
        filters: filters_tx,
        shutdown: shutdown_tx,
        events,
        stats: Arc::clone(&stats),
        stats_tx,
        feedback_interval: config.feedback_interval,
    };
    let outcome = supervisor.supervise(tasks, partition_by_task).await;
    close_client(client).await;

    Ok(RunSummary {
        stats: stats.snapshot(),
        end: outcome.run_end(),
        partitions: outcome.ends,
    })
}

/// Connects, selects the partitions to read, and binds the sink to the event
/// hub. When the selection or the bind is rejected, the client opened for it
/// is closed before the error returns.
async fn start_run<S: MessageSink>(
    config: &EngineConfig,
    sink: &S,
) -> anyhow::Result<(EventHubConnection, Vec<String>)> {
    let connection = connect(&config.target).await?;
    let partitions =
        match select_partitions(&connection.partition_ids, config.partitions.as_deref()) {
            Ok(partitions) => partitions,
            Err(error) => {
                let context = format!(
                    "Invalid partition selection for '{}/{}'",
                    connection.fully_qualified_namespace, connection.event_hub_name
                );
                close_client_quietly(connection.client).await;
                return Err(error.context(context));
            }
        };

    let identity = EventHubIdentity {
        fully_qualified_namespace: connection.fully_qualified_namespace.clone(),
        event_hub_name: connection.event_hub_name.clone(),
        created_on: connection.created_on,
    };
    if let Err(error) = bind_sink(sink, &identity).await {
        close_client_quietly(connection.client).await;
        return Err(error);
    }

    Ok((connection, partitions))
}

/// Binds `sink` to `hub`, naming the hub in the error when the sink refuses.
async fn bind_sink<S: MessageSink>(sink: &S, hub: &EventHubIdentity) -> anyhow::Result<()> {
    sink.bind(hub).await.with_context(|| {
        format!(
            "Failed to bind the archive to '{}/{}'",
            hub.fully_qualified_namespace, hub.event_hub_name
        )
    })
}

/// Picks the partitions to read: all of `available`, or the requested ones in
/// the requested order without repeats.
fn select_partitions(
    available: &[String],
    requested: Option<&[String]>,
) -> anyhow::Result<Vec<String>> {
    let Some(requested) = requested else {
        return Ok(available.to_vec());
    };
    if requested.is_empty() {
        anyhow::bail!("No partitions were selected");
    }

    let mut selected: Vec<String> = Vec::with_capacity(requested.len());
    for partition_id in requested {
        if !available.contains(partition_id) {
            anyhow::bail!(
                "Partition '{partition_id}' does not exist; the event hub has {available:?}"
            );
        }
        if !selected.contains(partition_id) {
            selected.push(partition_id.clone());
        }
    }
    Ok(selected)
}

/// Sends a control event, waiting for channel space.
pub(crate) async fn send_control(events: &mpsc::Sender<EngineEvent>, event: EngineEvent) {
    if events.send(event).await.is_err() {
        // Best-effort: a consumer that dropped its receiver no longer watches
        // the run, and capture into the sink continues without it.
        warn!("The engine event receiver is closed; a control event was not delivered");
    }
}

async fn close_client(client: Arc<ConsumerClient>) {
    match Arc::try_unwrap(client) {
        Ok(client) => close_client_quietly(client).await,
        Err(_) => warn!(
            "The Event Hubs client is still shared after every partition ended; its connection \
             is not closed explicitly (dropping the client does not close it)"
        ),
    }
}

/// Coordinates the partition tasks: applies commands, publishes statistics,
/// and collects each task's end.
struct Supervisor {
    commands: mpsc::Receiver<EngineCommand>,
    filters: watch::Sender<CaptureFilters>,
    shutdown: watch::Sender<bool>,
    events: mpsc::Sender<EngineEvent>,
    stats: Arc<StatsTracker>,
    stats_tx: watch::Sender<StatsSnapshot>,
    feedback_interval: Duration,
}

struct SupervisionOutcome {
    ends: BTreeMap<String, PartitionEnd>,
    stopped: bool,
}

impl SupervisionOutcome {
    fn run_end(&self) -> RunEnd {
        if self.stopped {
            return RunEnd::Stopped;
        }

        let failed: Vec<&str> = self
            .ends
            .iter()
            .filter(|(_, end)| matches!(end, PartitionEnd::Failed(_)))
            .map(|(partition_id, _)| partition_id.as_str())
            .collect();
        if failed.is_empty() {
            RunEnd::Completed
        } else if failed.len() == self.ends.len() {
            RunEnd::Failed("every partition failed".to_owned())
        } else {
            RunEnd::Failed(format!("partitions {} failed", failed.join(", ")))
        }
    }
}

impl Supervisor {
    async fn supervise(
        mut self,
        mut tasks: JoinSet<PartitionEnd>,
        partition_by_task: HashMap<Id, String>,
    ) -> SupervisionOutcome {
        let mut ends = BTreeMap::new();
        let mut stopped = false;
        let mut commands_open = true;
        let mut ticker = tokio::time::interval(self.feedback_interval.max(MIN_FEEDBACK_INTERVAL));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        while !tasks.is_empty() {
            tokio::select! {
                command = self.commands.recv(), if commands_open => match command {
                    Some(EngineCommand::UpdateFilters(filters)) => {
                        info!(filters = ?filters, "Applied new capture filters");
                        self.filters.send_replace(filters);
                    }
                    Some(EngineCommand::Stop) if !stopped => {
                        info!("Stop requested; ending every partition");
                        stopped = true;
                        self.shutdown.send_replace(true);
                    }
                    Some(EngineCommand::Stop) => {}
                    None => commands_open = false,
                },
                Some(joined) = tasks.join_next_with_id() => {
                    let (task_id, end, abnormal) = match joined {
                        Ok((task_id, end)) => (task_id, end, false),
                        Err(error) => (error.id(), PartitionEnd::Failed(format!("the partition task ended abnormally: {error}")), true),
                    };
                    let partition_id = partition_by_task
                        .get(&task_id)
                        .cloned()
                        .unwrap_or_else(|| format!("task {task_id}"));
                    if let (true, PartitionEnd::Failed(reason)) = (abnormal, &end) {
                        send_control(&self.events, EngineEvent::Error {
                            partition_id: Some(partition_id.clone()),
                            error: reason.clone(),
                            fatal: false,
                        })
                        .await;
                    }
                    send_control(&self.events, EngineEvent::PartitionEnded {
                        partition_id: partition_id.clone(),
                        end: end.clone(),
                    })
                    .await;
                    ends.insert(partition_id, end);
                },
                _ = ticker.tick() => {
                    self.stats_tx.send_replace(self.stats.snapshot());
                },
            }
        }

        self.stats_tx.send_replace(self.stats.snapshot());
        SupervisionOutcome { ends, stopped }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filters::TextFilter;

    struct SupervisorHarness {
        supervisor: Supervisor,
        commands: mpsc::Sender<EngineCommand>,
        events: mpsc::Receiver<EngineEvent>,
        filters: watch::Receiver<CaptureFilters>,
        shutdown: watch::Receiver<bool>,
        stats: watch::Receiver<StatsSnapshot>,
        tracker: Arc<StatsTracker>,
    }

    fn supervisor_harness() -> SupervisorHarness {
        let (commands_tx, commands_rx) = mpsc::channel(4);
        let (events_tx, events_rx) = mpsc::channel(16);
        let (filters_tx, filters_rx) = watch::channel(CaptureFilters::default());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let tracker = Arc::new(StatsTracker::new(Utc::now()));
        let (stats_tx, stats_rx) = watch::channel(tracker.snapshot());
        SupervisorHarness {
            supervisor: Supervisor {
                commands: commands_rx,
                filters: filters_tx,
                shutdown: shutdown_tx,
                events: events_tx,
                stats: Arc::clone(&tracker),
                stats_tx,
                feedback_interval: Duration::ZERO,
            },
            commands: commands_tx,
            events: events_rx,
            filters: filters_rx,
            shutdown: shutdown_rx,
            stats: stats_rx,
            tracker,
        }
    }

    fn spawn_ending(
        tasks: &mut JoinSet<PartitionEnd>,
        names: &mut HashMap<Id, String>,
        partition_id: &str,
        end: PartitionEnd,
    ) {
        let handle = tasks.spawn(async move { end });
        names.insert(handle.id(), partition_id.to_owned());
    }

    fn spawn_until_stop(
        tasks: &mut JoinSet<PartitionEnd>,
        names: &mut HashMap<Id, String>,
        partition_id: &str,
        mut shutdown: watch::Receiver<bool>,
    ) {
        let handle = tasks.spawn(async move {
            drop(shutdown.wait_for(|stop| *stop).await);
            PartitionEnd::Stopped
        });
        names.insert(handle.id(), partition_id.to_owned());
    }

    fn partition_ended(events: &mut mpsc::Receiver<EngineEvent>) -> Vec<(String, PartitionEnd)> {
        let mut ended = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let EngineEvent::PartitionEnded { partition_id, end } = event {
                ended.push((partition_id, end));
            }
        }
        ended.sort_by(|left, right| left.0.cmp(&right.0));
        ended
    }

    #[tokio::test]
    async fn partitions_ending_on_their_own_complete_the_run() {
        let mut h = supervisor_harness();
        let (mut tasks, mut names) = (JoinSet::new(), HashMap::new());
        spawn_ending(
            &mut tasks,
            &mut names,
            "0",
            PartitionEnd::NothingLeftInWindow,
        );
        spawn_ending(&mut tasks, &mut names, "1", PartitionEnd::UpperBoundPassed);

        let outcome = h.supervisor.supervise(tasks, names).await;

        assert_eq!(outcome.run_end(), RunEnd::Completed);
        assert_eq!(
            partition_ended(&mut h.events),
            vec![
                ("0".to_owned(), PartitionEnd::NothingLeftInWindow),
                ("1".to_owned(), PartitionEnd::UpperBoundPassed),
            ]
        );
    }

    #[tokio::test]
    async fn one_failed_partition_fails_the_run_and_names_it() {
        let h = supervisor_harness();
        let (mut tasks, mut names) = (JoinSet::new(), HashMap::new());
        spawn_ending(
            &mut tasks,
            &mut names,
            "0",
            PartitionEnd::NothingLeftInWindow,
        );
        spawn_ending(
            &mut tasks,
            &mut names,
            "1",
            PartitionEnd::Failed("boom".to_owned()),
        );

        let outcome = h.supervisor.supervise(tasks, names).await;

        assert_eq!(
            outcome.run_end(),
            RunEnd::Failed("partitions 1 failed".to_owned())
        );
        assert_eq!(outcome.ends["1"], PartitionEnd::Failed("boom".to_owned()));
    }

    #[test]
    fn a_stopped_run_stays_stopped_and_keeps_failed_partitions_visible() {
        let outcome = SupervisionOutcome {
            ends: BTreeMap::from([
                ("0".to_owned(), PartitionEnd::Stopped),
                ("1".to_owned(), PartitionEnd::Failed("boom".to_owned())),
            ]),
            stopped: true,
        };

        assert_eq!(outcome.run_end(), RunEnd::Stopped);
        assert!(matches!(outcome.ends["1"], PartitionEnd::Failed(_)));
    }

    #[tokio::test]
    async fn a_second_stop_and_a_closed_command_channel_change_nothing() {
        let h = supervisor_harness();
        let (mut tasks, mut names) = (JoinSet::new(), HashMap::new());
        spawn_until_stop(&mut tasks, &mut names, "0", h.shutdown.clone());
        h.commands.send(EngineCommand::Stop).await.unwrap();
        h.commands.send(EngineCommand::Stop).await.unwrap();
        drop(h.commands);

        let outcome = h.supervisor.supervise(tasks, names).await;

        assert_eq!(outcome.run_end(), RunEnd::Stopped);
        assert_eq!(outcome.ends["0"], PartitionEnd::Stopped);
    }

    #[tokio::test]
    async fn a_closed_command_channel_does_not_end_the_run_early() {
        let h = supervisor_harness();
        let (mut tasks, mut names) = (JoinSet::new(), HashMap::new());
        let (finish, finished) = tokio::sync::oneshot::channel::<()>();
        let handle = tasks.spawn(async move {
            drop(finished.await);
            PartitionEnd::NothingLeftInWindow
        });
        names.insert(handle.id(), "0".to_owned());
        drop(h.commands);
        let supervise = tokio::spawn(h.supervisor.supervise(tasks, names));

        tokio::task::yield_now().await;
        finish.send(()).unwrap();
        let outcome = supervise.await.unwrap();

        assert_eq!(outcome.run_end(), RunEnd::Completed);
    }

    #[tokio::test]
    async fn every_partition_failing_fails_the_run() {
        let h = supervisor_harness();
        let (mut tasks, mut names) = (JoinSet::new(), HashMap::new());
        spawn_ending(
            &mut tasks,
            &mut names,
            "0",
            PartitionEnd::Failed("a".to_owned()),
        );
        spawn_ending(
            &mut tasks,
            &mut names,
            "1",
            PartitionEnd::Failed("b".to_owned()),
        );

        let outcome = h.supervisor.supervise(tasks, names).await;

        assert!(matches!(outcome.run_end(), RunEnd::Failed(_)));
    }

    #[tokio::test]
    async fn a_stop_command_reaches_every_partition_and_stops_the_run() {
        let h = supervisor_harness();
        let (mut tasks, mut names) = (JoinSet::new(), HashMap::new());
        spawn_until_stop(&mut tasks, &mut names, "0", h.shutdown.clone());
        spawn_until_stop(&mut tasks, &mut names, "1", h.shutdown.clone());
        h.commands.send(EngineCommand::Stop).await.unwrap();

        let outcome = h.supervisor.supervise(tasks, names).await;

        assert_eq!(outcome.run_end(), RunEnd::Stopped);
        assert!(
            outcome
                .ends
                .values()
                .all(|end| *end == PartitionEnd::Stopped)
        );
        assert!(*h.shutdown.borrow());
    }

    #[tokio::test]
    async fn a_filter_update_reaches_the_partitions() {
        let h = supervisor_harness();
        let (mut tasks, mut names) = (JoinSet::new(), HashMap::new());
        spawn_until_stop(&mut tasks, &mut names, "0", h.shutdown.clone());
        let filters = CaptureFilters {
            time: None,
            text: Some(TextFilter::Contains(vec!["x".to_owned()])),
        };
        h.commands
            .send(EngineCommand::UpdateFilters(filters.clone()))
            .await
            .unwrap();
        h.commands.send(EngineCommand::Stop).await.unwrap();

        h.supervisor.supervise(tasks, names).await;

        assert_eq!(*h.filters.borrow(), filters);
    }

    #[tokio::test]
    async fn a_panicking_partition_task_is_reported_as_failed() {
        let mut h = supervisor_harness();
        let (mut tasks, mut names) = (JoinSet::new(), HashMap::new());
        let handle = tasks.spawn(async { panic!("partition task bug") });
        names.insert(handle.id(), "0".to_owned());

        let outcome = h.supervisor.supervise(tasks, names).await;

        assert!(
            matches!(&outcome.ends["0"], PartitionEnd::Failed(reason) if reason.contains("abnormally"))
        );
        let first = h.events.try_recv().unwrap();
        assert!(matches!(
            first,
            EngineEvent::Error { partition_id: Some(id), fatal: false, .. } if id == "0"
        ));
    }

    #[tokio::test]
    async fn the_final_statistics_are_published() {
        let mut h = supervisor_harness();
        let (mut tasks, mut names) = (JoinSet::new(), HashMap::new());
        h.tracker.record_stored();
        spawn_ending(
            &mut tasks,
            &mut names,
            "0",
            PartitionEnd::NothingLeftInWindow,
        );

        h.supervisor.supervise(tasks, names).await;

        assert_eq!(h.stats.borrow_and_update().stored, 1);
    }

    #[test]
    fn select_partitions_defaults_to_all() {
        let available = vec!["0".to_owned(), "1".to_owned()];

        assert_eq!(select_partitions(&available, None).unwrap(), available);
    }

    #[test]
    fn select_partitions_keeps_requested_order_without_repeats() {
        let available = vec!["0".to_owned(), "1".to_owned(), "2".to_owned()];
        let requested = vec!["2".to_owned(), "0".to_owned(), "2".to_owned()];

        let selected = select_partitions(&available, Some(&requested)).unwrap();

        assert_eq!(selected, vec!["2".to_owned(), "0".to_owned()]);
    }

    #[test]
    fn select_partitions_rejects_an_unknown_partition() {
        let available = vec!["0".to_owned()];

        let error = select_partitions(&available, Some(&["7".to_owned()])).unwrap_err();

        assert!(
            error.to_string().contains("'7' does not exist"),
            "{error:#}"
        );
    }

    #[test]
    fn select_partitions_rejects_an_empty_selection() {
        let error = select_partitions(&["0".to_owned()], Some(&[])).unwrap_err();

        assert!(error.to_string().contains("No partitions"), "{error:#}");
    }

    #[test]
    fn partition_end_renders_a_readable_reason() {
        assert_eq!(
            PartitionEnd::Failed("disk full".to_owned()).to_string(),
            "failed: disk full"
        );
    }

    #[tokio::test]
    async fn send_control_to_a_closed_receiver_returns_without_blocking() {
        let (events_tx, events_rx) = mpsc::channel(1);
        drop(events_rx);

        send_control(&events_tx, EngineEvent::Started { partitions: vec![] }).await;

        assert!(events_tx.is_closed());
    }

    #[tokio::test]
    async fn a_control_event_waits_for_space_instead_of_being_dropped() {
        let (events_tx, mut events_rx) = mpsc::channel(1);
        events_tx
            .try_send(EngineEvent::Started { partitions: vec![] })
            .unwrap();
        let sender = tokio::spawn(async move {
            send_control(
                &events_tx,
                EngineEvent::PartitionEnded {
                    partition_id: "0".to_owned(),
                    end: PartitionEnd::Stopped,
                },
            )
            .await;
        });

        let first = events_rx.recv().await.unwrap();
        let second = events_rx.recv().await.unwrap();
        sender.await.unwrap();

        assert!(matches!(first, EngineEvent::Started { .. }));
        assert!(matches!(second, EngineEvent::PartitionEnded { .. }));
    }

    struct NeverUsedSink;

    impl MessageSink for NeverUsedSink {
        async fn bind(&self, _hub: &EventHubIdentity) -> anyhow::Result<()> {
            anyhow::bail!("a run that cannot start never binds")
        }

        async fn store(
            &self,
            _message: &CapturedMessage,
            _checkpoint: &Checkpoint,
            _dedup: bool,
        ) -> anyhow::Result<StoreOutcome> {
            anyhow::bail!("a run that cannot start never stores")
        }

        async fn load_checkpoint(&self, _partition_id: &str) -> anyhow::Result<Option<Checkpoint>> {
            anyhow::bail!("a run that cannot start never loads checkpoints")
        }
    }

    #[derive(Default)]
    struct BindingSink {
        bound: std::sync::Mutex<Vec<EventHubIdentity>>,
        refuse: bool,
    }

    impl MessageSink for BindingSink {
        async fn bind(&self, hub: &EventHubIdentity) -> anyhow::Result<()> {
            if self.refuse {
                anyhow::bail!("this archive belongs to another event hub");
            }
            self.bound.lock().unwrap().push(hub.clone());
            Ok(())
        }

        async fn store(
            &self,
            _message: &CapturedMessage,
            _checkpoint: &Checkpoint,
            _dedup: bool,
        ) -> anyhow::Result<StoreOutcome> {
            anyhow::bail!("binding never stores")
        }

        async fn load_checkpoint(&self, _partition_id: &str) -> anyhow::Result<Option<Checkpoint>> {
            anyhow::bail!("binding never loads checkpoints")
        }
    }

    fn orders_hub() -> EventHubIdentity {
        EventHubIdentity {
            fully_qualified_namespace: "orders.servicebus.windows.net".to_owned(),
            event_hub_name: "orders".to_owned(),
            created_on: chrono::DateTime::from_timestamp(1_782_993_600, 0),
        }
    }

    #[tokio::test]
    async fn bind_sink_hands_the_identity_to_the_sink() {
        let sink = BindingSink::default();

        bind_sink(&sink, &orders_hub()).await.unwrap();

        assert_eq!(*sink.bound.lock().unwrap(), vec![orders_hub()]);
    }

    #[tokio::test]
    async fn bind_sink_names_the_event_hub_when_the_sink_refuses() {
        let sink = BindingSink {
            refuse: true,
            ..BindingSink::default()
        };

        let error = bind_sink(&sink, &orders_hub()).await.unwrap_err();

        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("'orders.servicebus.windows.net/orders'"),
            "{rendered}"
        );
        assert!(rendered.contains("another event hub"), "{rendered}");
        assert!(sink.bound.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_run_that_cannot_start_reports_a_fatal_error_and_fails() {
        let config = EngineConfig {
            target: EventHubTarget {
                auth: shared_azure::auth::AuthMethod::Credential {
                    kind: shared_azure::auth::CredentialKind::DeveloperTools,
                },
                client_secret: None,
                fully_qualified_namespace: Some("evil.example.com".to_owned()),
                event_hub_name: Some("orders".to_owned()),
                consumer_group: crate::connection::DEFAULT_CONSUMER_GROUP.to_owned(),
            },
            partitions: None,
            initial_filters: CaptureFilters::default(),
            ignore_checkpoint: false,
            prefetch: None,
            feedback_interval: Duration::from_secs(1),
        };
        let (mut channels, run) = CaptureEngine::build(config, NeverUsedSink);

        let error = run.await.unwrap_err();

        assert!(
            format!("{error:#}").contains("evil.example.com"),
            "{error:#}"
        );
        let event = channels.events.recv().await.unwrap();
        assert!(matches!(
            event,
            EngineEvent::Error {
                partition_id: None,
                fatal: true,
                ..
            }
        ));
        assert!(channels.events.recv().await.is_none());
    }
}
