//! One partition's capture task: snapshot, start position, receiver, and the
//! per-event pipeline.

use crate::engine::start::{
    PartitionSnapshot, StartDecision, checkpoint_ahead_of_partition, checkpoint_floor,
    choose_start, decide, initial_high_water, to_start_position, window_before_checkpoint,
};
use crate::engine::{Checkpoint, EngineEvent, MessageSink, PartitionEnd, SkipReason, send_control};
use crate::filters::CaptureFilters;
use crate::models::{CapturedMessage, captured_from, system_time_to_utc};
use crate::stats::StatsTracker;
use anyhow::Context;
use azure_messaging_eventhubs::error::ErrorKind;
use azure_messaging_eventhubs::models::ReceivedEventData;
use azure_messaging_eventhubs::{
    ConsumerClient, EventHubsError, EventReceiver, OpenReceiverOptions,
};
use chrono::Utc;
use futures_util::{Stream, StreamExt};
use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, trace, warn};

const CONSUMER_DISCONNECTED_MESSAGE: &str = "Another consumer with an owner level took this \
     partition; use a dedicated consumer group for this tool";

/// What one partition task needs from the engine.
pub(crate) struct PartitionContext<S> {
    pub(crate) partition_id: String,
    pub(crate) sink: Arc<S>,
    pub(crate) events: mpsc::Sender<EngineEvent>,
    pub(crate) stats: Arc<StatsTracker>,
    pub(crate) filters: watch::Receiver<CaptureFilters>,
    pub(crate) shutdown: watch::Receiver<bool>,
    pub(crate) ignore_checkpoint: bool,
    pub(crate) prefetch: Option<u32>,
    /// The stored checkpoint an `ignore_checkpoint` run must not move
    /// backwards; see [`checkpoint_floor`].
    pub(crate) checkpoint_floor: Option<Checkpoint>,
}

impl<S> PartitionContext<S> {
    /// Sends a message or skip event without waiting: when the consumer lags
    /// the event is dropped and counted, so capture never stalls behind a
    /// slow view.
    fn emit_data(&self, event: EngineEvent) {
        if self.events.try_send(event).is_err() {
            self.stats.record_ui_dropped();
        }
    }
}

enum Step {
    Continue,
    End(PartitionEnd),
}

/// Runs one partition until its window closes, a stop is requested, or it
/// fails. A failure is reported as an `Error` event and returned as
/// [`PartitionEnd::Failed`]; it never stops the other partitions.
pub(crate) async fn run_partition<S: MessageSink>(
    client: Arc<ConsumerClient>,
    mut context: PartitionContext<S>,
) -> PartitionEnd {
    let result = capture_partition(&client, &mut context).await;
    report_partition_end(&context, result).await
}

/// Turns a capture result into the partition's end, reporting a failure to
/// the consumer as a non-fatal `Error` event first.
async fn report_partition_end<S>(
    context: &PartitionContext<S>,
    result: anyhow::Result<PartitionEnd>,
) -> PartitionEnd {
    let end = match result {
        Ok(end) => end,
        Err(error) => {
            let message = format!("{error:#}");
            send_control(
                &context.events,
                EngineEvent::Error {
                    partition_id: Some(context.partition_id.clone()),
                    error: message.clone(),
                    fatal: false,
                },
            )
            .await;
            PartitionEnd::Failed(message)
        }
    };
    info!(partition_id = %context.partition_id, end = %end, "Partition capture ended");
    end
}

/// A partition ready to consume: its receiver and the replay-guard seed.
struct ReadyPartition {
    receiver: EventReceiver,
    high_water: Option<i64>,
    tail: Option<i64>,
}

async fn capture_partition<S: MessageSink>(
    client: &ConsumerClient,
    context: &mut PartitionContext<S>,
) -> anyhow::Result<PartitionEnd> {
    let mut shutdown = context.shutdown.clone();
    let Some(prepared) = until_stopped(&mut shutdown, prepare_partition(client, context)).await
    else {
        return Ok(PartitionEnd::Stopped);
    };
    let Some(ready) = prepared? else {
        return Ok(PartitionEnd::NothingLeftInWindow);
    };

    let result = consume_stream(
        ready.receiver.stream_events(),
        context,
        ready.high_water,
        ready.tail,
    )
    .await;
    if let Err(error) = ready.receiver.close().await {
        // Best-effort cleanup: the capture result is already decided, and the
        // client's close tears the connection down regardless.
        warn!(
            partition_id = %context.partition_id,
            error = %error,
            "Failed to close the partition receiver"
        );
    }

    result
}

/// Loads the checkpoint, snapshots the partition, picks the start, and opens
/// the receiver. Returns `None` when a closed time window leaves nothing to
/// read. Sets the context's checkpoint floor for `ignore_checkpoint` runs.
async fn prepare_partition<S: MessageSink>(
    client: &ConsumerClient,
    context: &mut PartitionContext<S>,
) -> anyhow::Result<Option<ReadyPartition>> {
    let partition_id = context.partition_id.clone();
    let checkpoint = context
        .sink
        .load_checkpoint(&partition_id)
        .await
        .with_context(|| format!("Failed to load the checkpoint of partition {partition_id}"))?;
    let taken_at = Utc::now();
    let properties = client
        .get_partition_properties(&partition_id)
        .await
        .with_context(|| format!("Failed to read the properties of partition {partition_id}"))?;
    let snapshot = PartitionSnapshot {
        is_empty: properties.is_empty,
        last_sequence_number: properties.last_enqueued_sequence_number,
        last_enqueued_time: properties
            .last_enqueued_time_utc
            .and_then(system_time_to_utc),
        taken_at,
    };
    let filters = context.filters.borrow().clone();

    if let Some(checkpoint) = checkpoint.as_ref().filter(|_| !context.ignore_checkpoint) {
        warn_about_checkpoint(&partition_id, checkpoint, &snapshot, &filters);
    }
    context.checkpoint_floor =
        checkpoint_floor(checkpoint.as_ref(), &snapshot, context.ignore_checkpoint);
    let choice = choose_start(checkpoint.as_ref(), &filters, context.ignore_checkpoint);
    debug!(partition_id = %partition_id, start = ?choice, "Chose the partition start position");

    let tail = match decide(&choice, &snapshot, filters.upper_time_bound()) {
        StartDecision::NothingInWindow => return Ok(None),
        StartDecision::Consume { tail } => tail,
    };

    let options = OpenReceiverOptions {
        start_position: Some(to_start_position(&choice)),
        prefetch: context.prefetch,
        ..Default::default()
    };
    let receiver = client
        .open_receiver_on_partition(partition_id.clone(), Some(options))
        .await
        .with_context(|| format!("Failed to open a receiver on partition {partition_id}"))?;

    Ok(Some(ReadyPartition {
        receiver,
        high_water: initial_high_water(&choice),
        tail,
    }))
}

/// Runs `work` unless a stop is requested first; returns `None` on a stop.
/// The SDK retries a failing setup call for about a minute, so setup must not
/// hold a stop request hostage.
async fn until_stopped<F: Future>(
    shutdown: &mut watch::Receiver<bool>,
    work: F,
) -> Option<F::Output> {
    tokio::select! {
        biased;
        () = stop_requested(shutdown) => None,
        output = work => Some(output),
    }
}

fn warn_about_checkpoint(
    partition_id: &str,
    checkpoint: &Checkpoint,
    snapshot: &PartitionSnapshot,
    filters: &CaptureFilters,
) {
    if checkpoint_ahead_of_partition(checkpoint, snapshot) {
        warn!(
            partition_id = %partition_id,
            checkpoint = checkpoint.sequence_number,
            last_sequence_number = snapshot.last_sequence_number,
            "The checkpoint is ahead of the partition (was the event hub recreated?); ignore the \
             checkpoint to read from the start"
        );
    }
    if window_before_checkpoint(checkpoint, filters.upper_time_bound()) {
        warn!(
            partition_id = %partition_id,
            checkpoint = checkpoint.sequence_number,
            "The time window ends before the checkpoint, so resuming reads none of it; ignore \
             the checkpoint, or search the stored archive instead"
        );
    }
}

/// Drives the per-event pipeline over `stream` until an early-stop rule, a
/// stop request, or an error ends it.
///
/// `high_water` is the largest sequence number already processed (seeded
/// from the checkpoint the receiver resumed after); the SDK re-attaches with
/// the original start position after a reconnect, so events at or below it
/// are replays and are skipped before reaching the sink. With `tail` set, the
/// partition ends after processing the event at or past that sequence number.
pub(crate) async fn consume_stream<S, St>(
    stream: St,
    context: &mut PartitionContext<S>,
    mut high_water: Option<i64>,
    tail: Option<i64>,
) -> anyhow::Result<PartitionEnd>
where
    S: MessageSink,
    St: Stream<Item = Result<ReceivedEventData, EventHubsError>>,
{
    let mut stream = pin!(stream);
    let mut filters = context.filters.borrow_and_update().clone();

    loop {
        let item = tokio::select! {
            biased;
            () = stop_requested(&mut context.shutdown) => return Ok(PartitionEnd::Stopped),
            item = stream.next() => item,
        };
        let event = match item {
            Some(Ok(event)) => event,
            Some(Err(error)) => return Err(stream_error(error, &context.partition_id)),
            None => anyhow::bail!(
                "The event stream of partition {} ended without an error",
                context.partition_id
            ),
        };

        if matches!(context.filters.has_changed(), Ok(true)) {
            filters = context.filters.borrow_and_update().clone();
        }
        match process_event(context, &filters, &event, &mut high_water, tail).await? {
            Step::Continue => {}
            Step::End(end) => return Ok(end),
        }
    }
}

/// Resolves once a stop is requested. A dropped sender means the engine is
/// gone, which is a stop as well, so both outcomes of the wait end here.
async fn stop_requested(shutdown: &mut watch::Receiver<bool>) {
    drop(shutdown.wait_for(|stop| *stop).await);
}

async fn process_event<S: MessageSink>(
    context: &PartitionContext<S>,
    filters: &CaptureFilters,
    event: &ReceivedEventData,
    high_water: &mut Option<i64>,
    tail: Option<i64>,
) -> anyhow::Result<Step> {
    let partition_id = &context.partition_id;
    let received_at = Utc::now();
    let Some(message) = captured_from(partition_id, event, received_at) else {
        context
            .stats
            .record_received(partition_id, received_at, None);
        context.stats.record_unkeyable();
        debug!(partition_id = %partition_id, "Skipping an event without a sequence number");
        context.emit_data(skipped(partition_id, SkipReason::Unkeyable));
        return Ok(Step::Continue);
    };
    context
        .stats
        .record_received(partition_id, received_at, message.enqueued_time);

    let sequence_number = message.key.sequence_number;
    if high_water.is_some_and(|processed| sequence_number <= processed) {
        context.stats.record_duplicate();
        trace!(partition_id = %partition_id, sequence_number, "Skipping a replayed event");
        context.emit_data(skipped(partition_id, SkipReason::Duplicate));
        return Ok(Step::Continue);
    }

    let checkpoint = match &context.checkpoint_floor {
        Some(floor) if sequence_number <= floor.sequence_number => floor.clone(),
        _ => Checkpoint {
            partition_id: partition_id.clone(),
            sequence_number,
            offset: message.offset.clone(),
            enqueued_time: message.enqueued_time,
            updated_at: received_at,
        },
    };
    let outcome = context
        .sink
        .store(&message, &checkpoint, !context.ignore_checkpoint)
        .await
        .with_context(|| {
            format!("Failed to store event {sequence_number} of partition {partition_id}")
        })?;
    *high_water = Some(sequence_number);
    trace!(
        partition_id = %partition_id,
        sequence_number,
        newly_stored = outcome.newly_stored,
        "Stored the event and its checkpoint"
    );

    let past_upper_bound = filters
        .upper_time_bound()
        .zip(message.enqueued_time)
        .is_some_and(|(upper, enqueued)| enqueued > upper);
    emit_outcome(context, filters, message, outcome.newly_stored);

    if past_upper_bound {
        return Ok(Step::End(PartitionEnd::UpperBoundPassed));
    }
    if tail.is_some_and(|tail| sequence_number >= tail) {
        return Ok(Step::End(PartitionEnd::NothingLeftInWindow));
    }
    Ok(Step::Continue)
}

fn emit_outcome<S>(
    context: &PartitionContext<S>,
    filters: &CaptureFilters,
    message: CapturedMessage,
    newly_stored: bool,
) {
    let partition_id = &context.partition_id;
    if !newly_stored {
        context.stats.record_duplicate();
        context.emit_data(skipped(partition_id, SkipReason::Duplicate));
        return;
    }

    context.stats.record_stored();
    match skip_reason(filters, &message) {
        None => context.emit_data(EngineEvent::Message(Box::new(message))),
        Some(reason) => {
            context.stats.record_filtered();
            context.emit_data(skipped(partition_id, reason));
        }
    }
}

fn skip_reason(filters: &CaptureFilters, message: &CapturedMessage) -> Option<SkipReason> {
    if filters.time.is_some() && message.enqueued_time.is_none() {
        Some(SkipReason::NoEnqueuedTime)
    } else if !filters.matches(message) {
        Some(SkipReason::Filtered)
    } else {
        None
    }
}

fn skipped(partition_id: &str, reason: SkipReason) -> EngineEvent {
    EngineEvent::Skipped {
        partition_id: partition_id.to_owned(),
        reason,
    }
}

fn stream_error(error: EventHubsError, partition_id: &str) -> anyhow::Error {
    let context = match &error.kind {
        ErrorKind::ConsumerDisconnected(_) => CONSUMER_DISCONNECTED_MESSAGE.to_owned(),
        _ => format!("The event stream of partition {partition_id} failed after the SDK's retries"),
    };

    anyhow::Error::new(error).context(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::StoreOutcome;
    use crate::filters::{TextFilter, TimeFilter};
    use crate::models::MessageKey;
    use crate::models::test_events::{annotations, event};
    use azure_core_amqp::AmqpMessage;
    use azure_core_amqp::message::AmqpMessageBody;
    use chrono::{DateTime, Duration};
    use futures_util::stream;
    use std::collections::HashSet;
    use std::sync::Mutex;

    type SdkResult = Result<ReceivedEventData, EventHubsError>;

    #[derive(Default)]
    struct FakeSink {
        stored: Mutex<Vec<(CapturedMessage, Checkpoint, bool)>>,
        known: Mutex<HashSet<MessageKey>>,
        fail_store: bool,
    }

    impl FakeSink {
        fn stored(&self) -> Vec<(CapturedMessage, Checkpoint, bool)> {
            self.stored.lock().unwrap().clone()
        }

        fn stored_sequence_numbers(&self) -> Vec<i64> {
            self.stored()
                .iter()
                .map(|(message, _, _)| message.key.sequence_number)
                .collect()
        }
    }

    impl MessageSink for FakeSink {
        async fn store(
            &self,
            message: &CapturedMessage,
            checkpoint: &Checkpoint,
            dedup: bool,
        ) -> anyhow::Result<StoreOutcome> {
            if self.fail_store {
                anyhow::bail!("disk full");
            }
            let first_time = self.known.lock().unwrap().insert(message.key.clone());
            self.stored
                .lock()
                .unwrap()
                .push((message.clone(), checkpoint.clone(), dedup));
            Ok(StoreOutcome {
                newly_stored: first_time || !dedup,
            })
        }

        async fn load_checkpoint(&self, _partition_id: &str) -> anyhow::Result<Option<Checkpoint>> {
            Ok(None)
        }
    }

    struct Harness {
        context: PartitionContext<FakeSink>,
        sink: Arc<FakeSink>,
        events: mpsc::Receiver<EngineEvent>,
        stats: Arc<StatsTracker>,
        filters: watch::Sender<CaptureFilters>,
        shutdown: watch::Sender<bool>,
    }

    fn harness(sink: FakeSink, filters: CaptureFilters, capacity: usize) -> Harness {
        let sink = Arc::new(sink);
        let (events_tx, events) = mpsc::channel(capacity);
        let stats = Arc::new(StatsTracker::new(Utc::now()));
        let (filters_tx, filters_rx) = watch::channel(filters);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Harness {
            context: PartitionContext {
                partition_id: "0".to_owned(),
                sink: Arc::clone(&sink),
                events: events_tx,
                stats: Arc::clone(&stats),
                filters: filters_rx,
                shutdown: shutdown_rx,
                ignore_checkpoint: false,
                prefetch: None,
                checkpoint_floor: None,
            },
            sink,
            events,
            stats,
            filters: filters_tx,
            shutdown: shutdown_tx,
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp_millis(Utc::now().timestamp_millis()).unwrap()
    }

    fn ok(sequence_number: i64, body: &str) -> SdkResult {
        Ok(event(sequence_number, now(), body))
    }

    fn drain(events: &mut mpsc::Receiver<EngineEvent>) -> Vec<EngineEvent> {
        let mut drained = Vec::new();
        while let Ok(event) = events.try_recv() {
            drained.push(event);
        }
        drained
    }

    fn message_sequence_numbers(events: &[EngineEvent]) -> Vec<i64> {
        events
            .iter()
            .filter_map(|event| match event {
                EngineEvent::Message(message) => Some(message.key.sequence_number),
                _ => None,
            })
            .collect()
    }

    fn skip_reasons(events: &[EngineEvent]) -> Vec<SkipReason> {
        events
            .iter()
            .filter_map(|event| match event {
                EngineEvent::Skipped { reason, .. } => Some(*reason),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn events_are_stored_with_their_checkpoint_then_emitted() {
        let mut h = harness(FakeSink::default(), CaptureFilters::default(), 16);
        let events = stream::iter(vec![ok(1, "a"), ok(2, "b")]);

        let end = consume_stream(events, &mut h.context, None, Some(2))
            .await
            .unwrap();

        assert_eq!(end, PartitionEnd::NothingLeftInWindow);
        let stored = h.sink.stored();
        assert_eq!(h.sink.stored_sequence_numbers(), vec![1, 2]);
        assert_eq!(stored[0].1.sequence_number, 1);
        assert_eq!(stored[0].1.offset.as_deref(), Some("offset-1"));
        assert_eq!(stored[0].1.enqueued_time, stored[0].0.enqueued_time);
        assert!(stored.iter().all(|(_, _, dedup)| *dedup));
        assert_eq!(message_sequence_numbers(&drain(&mut h.events)), vec![1, 2]);
        assert_eq!(h.stats.snapshot().stored, 2);
    }

    #[tokio::test]
    async fn a_duplicate_from_the_sink_is_counted_and_not_emitted() {
        let sink = FakeSink::default();
        sink.known.lock().unwrap().insert(MessageKey {
            partition_id: "0".to_owned(),
            sequence_number: 1,
        });
        let mut h = harness(sink, CaptureFilters::default(), 16);

        let end = consume_stream(
            stream::iter(vec![ok(1, "a")]),
            &mut h.context,
            None,
            Some(1),
        )
        .await
        .unwrap();

        assert_eq!(end, PartitionEnd::NothingLeftInWindow);
        let events = drain(&mut h.events);
        assert!(message_sequence_numbers(&events).is_empty());
        assert_eq!(skip_reasons(&events), vec![SkipReason::Duplicate]);
        assert_eq!(h.stats.snapshot().duplicated, 1);
    }

    #[tokio::test]
    async fn a_filtered_event_is_stored_but_not_emitted() {
        let filters = CaptureFilters {
            time: None,
            text: Some(TextFilter::Contains(vec!["keep".to_owned()])),
        };
        let mut h = harness(FakeSink::default(), filters, 16);
        let events = stream::iter(vec![ok(1, "drop me"), ok(2, "keep me")]);

        consume_stream(events, &mut h.context, None, Some(2))
            .await
            .unwrap();

        assert_eq!(h.sink.stored_sequence_numbers(), vec![1, 2]);
        let events = drain(&mut h.events);
        assert_eq!(message_sequence_numbers(&events), vec![2]);
        assert_eq!(skip_reasons(&events), vec![SkipReason::Filtered]);
        assert_eq!(h.stats.snapshot().filtered_out, 1);
    }

    #[tokio::test]
    async fn an_event_without_enqueued_time_fails_an_active_time_filter() {
        let filters = CaptureFilters {
            time: Some(TimeFilter::NewerThan(now() - Duration::days(1))),
            text: None,
        };
        let mut h = harness(FakeSink::default(), filters, 16);
        let message = AmqpMessage::builder()
            .with_body(AmqpMessageBody::Binary(vec![b"x".to_vec()]))
            .with_message_annotations(annotations(Some(1), None))
            .build();
        let events = stream::iter(vec![Ok(ReceivedEventData::from(message))]);

        consume_stream(events, &mut h.context, None, Some(1))
            .await
            .unwrap();

        assert_eq!(
            skip_reasons(&drain(&mut h.events)),
            vec![SkipReason::NoEnqueuedTime]
        );
        assert_eq!(h.sink.stored_sequence_numbers(), vec![1]);
    }

    #[tokio::test]
    async fn replayed_events_are_skipped_before_the_sink() {
        let mut h = harness(FakeSink::default(), CaptureFilters::default(), 16);
        let events = stream::iter(vec![
            ok(1, "a"),
            ok(2, "b"),
            ok(1, "a"),
            ok(2, "b"),
            ok(3, "c"),
        ]);

        consume_stream(events, &mut h.context, None, Some(3))
            .await
            .unwrap();

        assert_eq!(h.sink.stored_sequence_numbers(), vec![1, 2, 3]);
        assert_eq!(
            message_sequence_numbers(&drain(&mut h.events)),
            vec![1, 2, 3]
        );
        assert_eq!(h.stats.snapshot().duplicated, 2);
    }

    #[tokio::test]
    async fn replay_guard_also_protects_an_ignored_checkpoint_run() {
        let mut h = harness(FakeSink::default(), CaptureFilters::default(), 16);
        h.context.ignore_checkpoint = true;
        let events = stream::iter(vec![ok(1, "a"), ok(2, "b"), ok(1, "a"), ok(2, "b")]);

        consume_stream(events, &mut h.context, None, Some(2))
            .await
            .unwrap();

        let stored = h.sink.stored();
        assert_eq!(h.sink.stored_sequence_numbers(), vec![1, 2]);
        assert!(stored.iter().all(|(_, _, dedup)| !dedup));
        assert_eq!(message_sequence_numbers(&drain(&mut h.events)), vec![1, 2]);
    }

    #[tokio::test]
    async fn a_seeded_high_water_mark_skips_events_up_to_the_checkpoint() {
        let mut h = harness(FakeSink::default(), CaptureFilters::default(), 16);
        let events = stream::iter(vec![ok(5, "old"), ok(6, "new")]);

        consume_stream(events, &mut h.context, Some(5), Some(6))
            .await
            .unwrap();

        assert_eq!(h.sink.stored_sequence_numbers(), vec![6]);
    }

    #[tokio::test]
    async fn an_unkeyable_event_is_counted_and_skipped() {
        let mut h = harness(FakeSink::default(), CaptureFilters::default(), 16);
        let message = AmqpMessage::builder()
            .with_body(AmqpMessageBody::Binary(vec![b"x".to_vec()]))
            .with_message_annotations(annotations(None, Some(now())))
            .build();
        let events = stream::iter(vec![Ok(ReceivedEventData::from(message)), ok(1, "a")]);

        consume_stream(events, &mut h.context, None, Some(1))
            .await
            .unwrap();

        assert_eq!(h.sink.stored_sequence_numbers(), vec![1]);
        assert_eq!(h.stats.snapshot().unkeyable, 1);
        assert_eq!(h.stats.snapshot().received, 2);
    }

    #[tokio::test]
    async fn an_event_past_the_upper_bound_is_stored_and_ends_the_partition() {
        let bound = now() - Duration::minutes(10);
        let filters = CaptureFilters {
            time: Some(TimeFilter::OlderOrEqual(bound)),
            text: None,
        };
        let mut h = harness(FakeSink::default(), filters, 16);
        let events = stream::iter(vec![
            Ok(event(1, bound - Duration::minutes(1), "inside")),
            Ok(event(2, bound + Duration::minutes(1), "outside")),
            Ok(event(3, bound + Duration::minutes(2), "never read")),
        ]);

        let end = consume_stream(events, &mut h.context, None, None)
            .await
            .unwrap();

        assert_eq!(end, PartitionEnd::UpperBoundPassed);
        assert_eq!(h.sink.stored_sequence_numbers(), vec![1, 2]);
        assert_eq!(message_sequence_numbers(&drain(&mut h.events)), vec![1]);
    }

    #[tokio::test]
    async fn a_filter_update_applies_to_later_events_only() {
        let mut h = harness(FakeSink::default(), CaptureFilters::default(), 16);
        let (feed, inbox) = mpsc::unbounded_channel::<SdkResult>();
        let events = stream::unfold(inbox, |mut inbox| async move {
            inbox.recv().await.map(|item| (item, inbox))
        });
        let mut context = h.context;
        let task = tokio::spawn(async move {
            let end = consume_stream(events, &mut context, None, Some(2)).await;
            (end, context)
        });

        feed.send(ok(1, "before")).unwrap();
        let first = h.events.recv().await.unwrap();
        h.filters.send_replace(CaptureFilters {
            time: None,
            text: Some(TextFilter::Contains(vec!["nothing matches".to_owned()])),
        });
        feed.send(ok(2, "after")).unwrap();
        let (end, _) = task.await.unwrap();

        assert_eq!(end.unwrap(), PartitionEnd::NothingLeftInWindow);
        assert!(matches!(first, EngineEvent::Message(message) if message.key.sequence_number == 1));
        assert_eq!(
            skip_reasons(&drain(&mut h.events)),
            vec![SkipReason::Filtered]
        );
    }

    #[tokio::test]
    async fn a_stop_request_ends_the_partition() {
        let mut h = harness(FakeSink::default(), CaptureFilters::default(), 16);
        h.shutdown.send_replace(true);

        let end = consume_stream(stream::pending::<SdkResult>(), &mut h.context, None, None)
            .await
            .unwrap();

        assert_eq!(end, PartitionEnd::Stopped);
    }

    #[tokio::test]
    async fn a_dropped_engine_counts_as_a_stop() {
        let mut h = harness(FakeSink::default(), CaptureFilters::default(), 16);
        drop(h.shutdown);

        let end = consume_stream(stream::pending::<SdkResult>(), &mut h.context, None, None)
            .await
            .unwrap();

        assert_eq!(end, PartitionEnd::Stopped);
    }

    #[tokio::test]
    async fn a_consumer_disconnected_error_names_the_fix() {
        let mut h = harness(FakeSink::default(), CaptureFilters::default(), 16);
        let failure = EventHubsError::from(ErrorKind::ConsumerDisconnected(None));
        let events = stream::iter(vec![ok(1, "a"), Err(failure)]);

        let error = consume_stream(events, &mut h.context, None, None)
            .await
            .unwrap_err();

        assert!(
            format!("{error:#}").contains("use a dedicated consumer group"),
            "{error:#}"
        );
        assert_eq!(h.sink.stored_sequence_numbers(), vec![1]);
    }

    #[tokio::test]
    async fn another_stream_error_fails_the_partition() {
        let mut h = harness(FakeSink::default(), CaptureFilters::default(), 16);
        let failure = EventHubsError::from(ErrorKind::SimpleMessage("link detached".into()));

        let error = consume_stream(stream::iter(vec![Err(failure)]), &mut h.context, None, None)
            .await
            .unwrap_err();

        let rendered = format!("{error:#}");
        assert!(rendered.contains("partition 0 failed"), "{rendered}");
        assert!(rendered.contains("link detached"), "{rendered}");
    }

    #[tokio::test]
    async fn a_stream_that_ends_without_an_error_fails_the_partition() {
        let mut h = harness(FakeSink::default(), CaptureFilters::default(), 16);

        let error = consume_stream(stream::iter(vec![ok(1, "a")]), &mut h.context, None, None)
            .await
            .unwrap_err();

        assert!(
            format!("{error:#}").contains("ended without an error"),
            "{error:#}"
        );
    }

    #[tokio::test]
    async fn a_sink_failure_fails_the_partition_and_emits_nothing() {
        let sink = FakeSink {
            fail_store: true,
            ..FakeSink::default()
        };
        let mut h = harness(sink, CaptureFilters::default(), 16);

        let error = consume_stream(
            stream::iter(vec![ok(1, "a")]),
            &mut h.context,
            None,
            Some(1),
        )
        .await
        .unwrap_err();

        assert!(format!("{error:#}").contains("disk full"), "{error:#}");
        assert!(drain(&mut h.events).is_empty());
        assert_eq!(h.stats.snapshot().stored, 0);
    }

    #[tokio::test]
    async fn a_full_event_channel_drops_view_events_but_keeps_capturing() {
        let mut h = harness(FakeSink::default(), CaptureFilters::default(), 1);
        let events = stream::iter(vec![ok(1, "a"), ok(2, "b"), ok(3, "c")]);

        consume_stream(events, &mut h.context, None, Some(3))
            .await
            .unwrap();

        assert_eq!(h.sink.stored_sequence_numbers(), vec![1, 2, 3]);
        assert_eq!(message_sequence_numbers(&drain(&mut h.events)), vec![1]);
        assert_eq!(h.stats.snapshot().ui_dropped, 2);
    }

    #[tokio::test]
    async fn an_ignored_checkpoint_run_never_moves_the_checkpoint_backwards() {
        let mut h = harness(FakeSink::default(), CaptureFilters::default(), 16);
        h.context.ignore_checkpoint = true;
        h.context.checkpoint_floor = Some(Checkpoint {
            partition_id: "0".to_owned(),
            sequence_number: 5,
            offset: Some("offset-5".to_owned()),
            enqueued_time: None,
            updated_at: now(),
        });
        let events = stream::iter(vec![ok(3, "old"), ok(5, "at"), ok(6, "new")]);

        consume_stream(events, &mut h.context, None, Some(6))
            .await
            .unwrap();

        let checkpoints: Vec<i64> = h
            .sink
            .stored()
            .iter()
            .map(|(_, checkpoint, _)| checkpoint.sequence_number)
            .collect();
        assert_eq!(h.sink.stored_sequence_numbers(), vec![3, 5, 6]);
        assert_eq!(checkpoints, vec![5, 5, 6]);
    }

    #[tokio::test]
    async fn a_failed_capture_is_reported_as_an_error_event_and_a_failed_end() {
        let mut h = harness(FakeSink::default(), CaptureFilters::default(), 16);

        let end = report_partition_end(&h.context, Err(anyhow::anyhow!("receiver refused"))).await;

        assert_eq!(end, PartitionEnd::Failed("receiver refused".to_owned()));
        let events = drain(&mut h.events);
        assert!(matches!(
            events.as_slice(),
            [EngineEvent::Error { partition_id: Some(id), error, fatal: false }]
                if id == "0" && error == "receiver refused"
        ));
    }

    #[tokio::test]
    async fn a_successful_capture_reports_its_end_without_events() {
        let mut h = harness(FakeSink::default(), CaptureFilters::default(), 16);

        let end = report_partition_end(&h.context, Ok(PartitionEnd::NothingLeftInWindow)).await;

        assert_eq!(end, PartitionEnd::NothingLeftInWindow);
        assert!(drain(&mut h.events).is_empty());
    }

    #[tokio::test]
    async fn a_stop_interrupts_setup_that_is_still_waiting() {
        let (stop, mut shutdown) = watch::channel(false);
        stop.send_replace(true);

        let outcome = until_stopped(&mut shutdown, std::future::pending::<()>()).await;

        assert!(outcome.is_none());
    }

    #[tokio::test]
    async fn setup_that_finishes_without_a_stop_returns_its_output() {
        let (_stop, mut shutdown) = watch::channel(false);

        let outcome = until_stopped(&mut shutdown, async { 7 }).await;

        assert_eq!(outcome, Some(7));
    }
}
