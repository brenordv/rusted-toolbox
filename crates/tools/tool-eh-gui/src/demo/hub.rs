//! A generated event hub with the engine's shape: `build` returns the same channels and run
//! future as `CaptureEngine::build`, so the bridge, the archive, and the state machine run as in
//! production. It binds the sink first, stores every message before emitting it, drops view
//! events on a full channel (counted as `ui_dropped`), and ends on Stop.

use super::Scenario;
use anyhow::Context;
use chrono::{DateTime, TimeDelta, Utc};
use eh_core::connection::EventHubIdentity;
use eh_core::engine::{
    Checkpoint, EVENT_CHANNEL_CAPACITY, EngineChannels, EngineCommand, EngineEvent, MessageSink,
    PartitionEnd, RunEnd, RunSummary,
};
use eh_core::filters::CaptureFilters;
use eh_core::models::{BodyKind, CapturedMessage, MessageKey};
use eh_core::stats::{StatsSnapshot, StatsTracker};
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, watch};
use tracing::debug;

pub const DEMO_NAMESPACE: &str = "demo.servicebus.windows.net";
pub const DEMO_HUB: &str = "demo";
/// When the demo hub was "created" (2026-01-01 UTC, Unix seconds): before any event it
/// generates, as a real hub's creation time is.
const DEMO_HUB_CREATED_ON: i64 = 1_767_225_600;
pub const PARTITIONS: [&str; 4] = ["0", "1", "2", "3"];
/// The partition the `partition-fail` scenario loses, and when.
const FAILING_PARTITION: &str = "2";
const FAIL_AFTER: Duration = Duration::from_secs(5);
const FEEDBACK_INTERVAL: Duration = Duration::from_secs(1);
/// One body in this many goes past the inspector's 256 KiB cap.
const OVERSIZED_EVERY: u64 = 1_000;
const OVERSIZED_BYTES: usize = 300 * 1024;

/// Builds a run of `scenario` that stores into `sink` and emits what passes `filters`.
pub fn build<S: MessageSink>(
    scenario: Scenario,
    filters: CaptureFilters,
    sink: S,
) -> (
    EngineChannels,
    impl Future<Output = anyhow::Result<RunSummary>> + Send + 'static,
) {
    let (commands_tx, commands) = mpsc::channel(16);
    let (events_tx, events) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
    let tracker = Arc::new(StatsTracker::new(Utc::now()));
    let (stats_tx, stats) = watch::channel(tracker.snapshot());
    let channels = EngineChannels {
        commands: commands_tx,
        events,
        stats,
    };
    let hub = Hub {
        scenario,
        sink,
        events: events_tx,
        tracker,
        stats: stats_tx,
    };
    (channels, hub.run(commands, filters))
}

struct Hub<S> {
    scenario: Scenario,
    sink: S,
    events: mpsc::Sender<EngineEvent>,
    tracker: Arc<StatsTracker>,
    stats: watch::Sender<StatsSnapshot>,
}

impl<S: MessageSink> Hub<S> {
    async fn run(
        self,
        mut commands: mpsc::Receiver<EngineCommand>,
        mut filters: CaptureFilters,
    ) -> anyhow::Result<RunSummary> {
        if let Err(error) = self.start().await {
            self.control(EngineEvent::Error {
                partition_id: None,
                error: format!("{error:#}"),
                fatal: true,
            })
            .await;
            return Err(error);
        }
        self.control(EngineEvent::Started {
            partitions: PARTITIONS.iter().map(|id| (*id).to_owned()).collect(),
        })
        .await;

        let mut generator = Generator::new(self.scenario.seed());
        let mut ends = BTreeMap::new();
        let started = Instant::now();
        let mut pace = tokio::time::interval(self.scenario.pace().every);
        let mut feedback = tokio::time::interval(FEEDBACK_INTERVAL);
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    Some(EngineCommand::UpdateFilters(next)) => filters = next,
                    Some(EngineCommand::Stop) | None => break,
                },
                _ = feedback.tick() => {
                    self.stats.send_replace(self.tracker.snapshot());
                }
                _ = pace.tick() => {
                    let elapsed = started.elapsed();
                    if self.scenario == Scenario::PartitionFail
                        && elapsed >= FAIL_AFTER
                        && !ends.contains_key(FAILING_PARTITION)
                    {
                        self.fail_partition(&mut ends).await;
                    }
                    for _ in 0..self.scenario.pace().batch(elapsed) {
                        let message = generator.next(&ends);
                        self.store_and_emit(message, &filters).await?;
                    }
                }
            }
        }

        for partition_id in PARTITIONS {
            ends.entry(partition_id.to_owned())
                .or_insert(PartitionEnd::Stopped);
        }
        let stats = self.tracker.snapshot();
        self.stats.send_replace(stats.clone());
        Ok(RunSummary {
            stats,
            partitions: ends,
            end: RunEnd::Stopped,
        })
    }

    async fn start(&self) -> anyhow::Result<()> {
        self.sink.bind(&identity()).await.with_context(|| {
            format!("Failed to bind the archive to '{DEMO_NAMESPACE}/{DEMO_HUB}'")
        })?;
        if self.scenario == Scenario::StartFail {
            anyhow::bail!(
                "Failed to read the event hub properties of '{DEMO_NAMESPACE}/{DEMO_HUB}': the \
                 demo hub refuses this connection"
            );
        }
        Ok(())
    }

    async fn fail_partition(&self, ends: &mut BTreeMap<String, PartitionEnd>) {
        let reason = "the demo partition lost its link".to_owned();
        self.control(EngineEvent::Error {
            partition_id: Some(FAILING_PARTITION.to_owned()),
            error: reason.clone(),
            fatal: false,
        })
        .await;
        let end = PartitionEnd::Failed(reason);
        self.control(EngineEvent::PartitionEnded {
            partition_id: FAILING_PARTITION.to_owned(),
            end: end.clone(),
        })
        .await;
        ends.insert(FAILING_PARTITION.to_owned(), end);
    }

    async fn store_and_emit(
        &self,
        message: CapturedMessage,
        filters: &CaptureFilters,
    ) -> anyhow::Result<()> {
        self.tracker.record_received(
            &message.key.partition_id,
            message.processed_at,
            message.enqueued_time,
        );
        self.sink
            .store(&message, &checkpoint_of(&message), true)
            .await
            .context("Failed to store a demo message")?;
        self.tracker.record_stored();
        if !filters.matches(&message) {
            self.tracker.record_filtered();
            return Ok(());
        }
        match self
            .events
            .try_send(EngineEvent::Message(Box::new(message)))
        {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => self.tracker.record_ui_dropped(),
            Err(TrySendError::Closed(_)) => {
                debug!("the demo hub's consumer is gone; the archive keeps receiving");
            }
        }
        Ok(())
    }

    async fn control(&self, event: EngineEvent) {
        if self.events.send(event).await.is_err() {
            debug!("the demo hub's consumer is gone; a control event was not delivered");
        }
    }
}

/// The demo event hub the archive is bound to.
pub fn identity() -> EventHubIdentity {
    EventHubIdentity {
        fully_qualified_namespace: DEMO_NAMESPACE.to_owned(),
        event_hub_name: DEMO_HUB.to_owned(),
        created_on: DateTime::from_timestamp(DEMO_HUB_CREATED_ON, 0),
    }
}

/// The checkpoint that storing `message` records.
pub fn checkpoint_of(message: &CapturedMessage) -> Checkpoint {
    Checkpoint {
        partition_id: message.key.partition_id.clone(),
        sequence_number: message.key.sequence_number,
        offset: message.offset.clone(),
        enqueued_time: message.enqueued_time,
        updated_at: message.processed_at,
    }
}

/// `count` messages as `scenario`'s hub generates them.
pub fn generated(scenario: Scenario, count: usize) -> Vec<CapturedMessage> {
    let mut generator = Generator::new(scenario.seed());
    let ended = BTreeMap::new();
    (0..count).map(|_| generator.next(&ended)).collect()
}

/// Deterministic messages: JSON, text, lossy and missing bodies, per-partition sequence
/// numbers, enqueue times just before now.
struct Generator {
    state: u64,
    count: u64,
    sequences: [i64; PARTITIONS.len()],
}

impl Generator {
    fn new(seed: u64) -> Self {
        Self {
            state: seed | 1,
            count: 0,
            sequences: [0; PARTITIONS.len()],
        }
    }

    /// xorshift64: enough variety for a demo, and repeatable per scenario.
    fn random(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    fn next(&mut self, ended: &BTreeMap<String, PartitionEnd>) -> CapturedMessage {
        let open: Vec<usize> = (0..PARTITIONS.len())
            .filter(|index| !ended.contains_key(PARTITIONS[*index]))
            .collect();
        let index = open
            .get((self.random() % open.len().max(1) as u64) as usize)
            .copied()
            .unwrap_or(0);
        self.sequences[index] += 1;
        self.count += 1;
        let sequence_number = self.sequences[index];
        let now = Utc::now();
        let lag = TimeDelta::milliseconds((self.random() % 900) as i64);
        let (body, body_kind) = self.body();

        CapturedMessage {
            key: MessageKey {
                partition_id: PARTITIONS[index].to_owned(),
                sequence_number,
            },
            offset: Some((sequence_number * 512).to_string()),
            partition_key: Some(format!("customer-{}", self.random() % 40)),
            enqueued_time: Some(now - lag),
            processed_at: now,
            message_id: Some(format!("demo-{}", self.count)),
            correlation_id: None,
            content_type: Some("application/json".to_owned()),
            properties: BTreeMap::new(),
            body,
            body_kind,
        }
    }

    fn body(&mut self) -> (String, BodyKind) {
        let n = self.count;
        if n.is_multiple_of(OVERSIZED_EVERY) {
            return ("x".repeat(OVERSIZED_BYTES), BodyKind::Utf8);
        }
        match n % 50 {
            0 => (String::new(), BodyKind::Missing),
            1 => ("caf\u{FFFD} order".to_owned(), BodyKind::LossyUtf8),
            2..=9 => (format!("order {n} shipped"), BodyKind::Utf8),
            _ => (
                format!(
                    "{{\"orderId\":{n},\"amount\":{}.{:02},\"status\":\"placed\"}}",
                    self.random() % 500,
                    self.random() % 100
                ),
                BodyKind::Utf8,
            ),
        }
    }
}
