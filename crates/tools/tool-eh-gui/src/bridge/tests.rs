use super::*;
use chrono::Utc;
use eh_core::engine::{RunEnd, SkipReason};
use eh_core::stats::StatsTracker;
use std::collections::BTreeMap;
use tokio::sync::mpsc::UnboundedReceiver;

enum Recorded {
    Delivery(UiDelivery, oneshot::Sender<()>),
    Done(EngineDone),
}

struct RecordingPort(mpsc::UnboundedSender<Recorded>);

impl UiPort for RecordingPort {
    fn deliver(&self, delivery: UiDelivery, ack: oneshot::Sender<()>) {
        self.0.send(Recorded::Delivery(delivery, ack)).unwrap();
    }

    fn done(&self, done: EngineDone) {
        self.0.send(Recorded::Done(done)).unwrap();
    }
}

struct Harness {
    events: mpsc::Sender<EngineEvent>,
    stats: watch::Sender<StatsSnapshot>,
    commands: mpsc::Receiver<EngineCommand>,
    channels: EngineChannels,
    port: Arc<dyn UiPort>,
    recorded: UnboundedReceiver<Recorded>,
}

fn snapshot() -> StatsSnapshot {
    StatsTracker::new(Utc::now()).snapshot()
}

fn summary(end: RunEnd) -> RunSummary {
    RunSummary {
        stats: snapshot(),
        partitions: BTreeMap::new(),
        end,
    }
}

fn harness(capacity: usize) -> Harness {
    let (events, events_rx) = mpsc::channel(capacity);
    let (stats, stats_rx) = watch::channel(snapshot());
    let (commands_tx, commands) = mpsc::channel(2);
    let (recorder, recorded) = mpsc::unbounded_channel();
    Harness {
        events,
        stats,
        commands,
        channels: EngineChannels {
            commands: commands_tx,
            events: events_rx,
            stats: stats_rx,
        },
        port: Arc::new(RecordingPort(recorder)),
        recorded,
    }
}

fn skipped(index: usize) -> EngineEvent {
    EngineEvent::Skipped {
        partition_id: index.to_string(),
        reason: SkipReason::Filtered,
    }
}

fn index_of(event: &EngineEvent) -> usize {
    match event {
        EngineEvent::Skipped { partition_id, .. } => partition_id.parse().unwrap(),
        other => panic!("unexpected event {other:?}"),
    }
}

/// Engine output ends when the run future drops its senders, as the real engine does.
async fn run_until(
    events: mpsc::Sender<EngineEvent>,
    stats: watch::Sender<StatsSnapshot>,
    end: oneshot::Receiver<()>,
) -> anyhow::Result<RunSummary> {
    drop(end.await);
    drop((events, stats));
    Ok(summary(RunEnd::Stopped))
}

async fn finished_ok() -> anyhow::Result<ExportOutcome> {
    Ok(ExportOutcome::default())
}

/// Acks every delivery until the session ends; returns the deliveries and the done.
async fn collect(recorded: &mut UnboundedReceiver<Recorded>) -> (Vec<UiDelivery>, EngineDone) {
    let mut deliveries = Vec::new();
    loop {
        match recorded.recv().await.unwrap() {
            Recorded::Delivery(delivery, ack) => {
                ack.send(()).unwrap();
                deliveries.push(delivery);
            }
            Recorded::Done(done) => return (deliveries, done),
        }
    }
}

async fn next_delivery(
    recorded: &mut UnboundedReceiver<Recorded>,
) -> (UiDelivery, oneshot::Sender<()>) {
    match recorded.recv().await.unwrap() {
        Recorded::Delivery(delivery, ack) => (delivery, ack),
        Recorded::Done(_) => panic!("the session ended before a delivery"),
    }
}

#[tokio::test]
async fn a_thousand_events_arrive_in_order_in_batches_of_at_most_512() {
    let mut h = harness(1024);
    for index in 0..1000 {
        h.events.try_send(skipped(index)).unwrap();
    }
    let (end_tx, end) = oneshot::channel();
    end_tx.send(()).unwrap();

    spawn_session(
        &Handle::current(),
        7,
        std::future::ready(()),
        h.channels,
        run_until(h.events, h.stats, end),
        test_support::sink(finished_ok()),
        h.port,
    );
    let (deliveries, _) = collect(&mut h.recorded).await;

    assert!(deliveries.iter().all(|d| d.events.len() <= BATCH_MAX));
    let order: Vec<usize> = deliveries
        .iter()
        .flat_map(|d| d.events.iter().map(index_of))
        .collect();
    assert_eq!(order, (0..1000).collect::<Vec<_>>());
}

#[tokio::test]
async fn no_second_delivery_arrives_before_the_first_is_acked() {
    let mut h = harness(16);
    let (end_tx, end) = oneshot::channel();
    let events = h.events.clone();
    spawn_session(
        &Handle::current(),
        1,
        std::future::ready(()),
        h.channels,
        run_until(h.events, h.stats, end),
        test_support::sink(finished_ok()),
        h.port,
    );
    events.send(skipped(0)).await.unwrap();
    let (_, ack) = next_delivery(&mut h.recorded).await;

    events.send(skipped(1)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let early = h.recorded.try_recv().is_ok();
    ack.send(()).unwrap();
    let (second, second_ack) = next_delivery(&mut h.recorded).await;

    assert!(!early, "a delivery arrived before the ack");
    assert_eq!(second.events.iter().map(index_of).collect::<Vec<_>>(), [1]);
    second_ack.send(()).unwrap();
    drop(events);
    end_tx.send(()).unwrap();
}

#[tokio::test]
async fn a_withheld_ack_leaves_events_in_the_bounded_channel() {
    let mut h = harness(4);
    let (_end_tx, end) = oneshot::channel();
    let events = h.events.clone();
    spawn_session(
        &Handle::current(),
        1,
        std::future::ready(()),
        h.channels,
        run_until(h.events, h.stats, end),
        test_support::sink(finished_ok()),
        h.port,
    );
    events.send(skipped(0)).await.unwrap();
    let (_first, _held_ack) = next_delivery(&mut h.recorded).await;

    for index in 1..=4 {
        events.try_send(skipped(index)).unwrap();
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    let overflow = events.try_send(skipped(5));

    assert!(matches!(overflow, Err(TrySendError::Full(_))));
}

#[tokio::test]
async fn statistics_changes_coalesce_into_the_latest() {
    let mut h = harness(4);
    for received in 1..=3 {
        h.stats.send_modify(|stats| stats.received = received);
    }
    let (end_tx, end) = oneshot::channel();
    end_tx.send(()).unwrap();

    spawn_session(
        &Handle::current(),
        1,
        std::future::ready(()),
        h.channels,
        run_until(h.events, h.stats, end),
        test_support::sink(finished_ok()),
        h.port,
    );
    let (deliveries, _) = collect(&mut h.recorded).await;

    let seen: Vec<u64> = deliveries
        .iter()
        .filter_map(|d| d.stats.as_ref().map(|stats| stats.received))
        .collect();
    assert_eq!(seen, [3]);
}

#[tokio::test]
async fn done_comes_after_every_delivery_and_carries_a_failed_finish() {
    let mut h = harness(16);
    for index in 0..3 {
        h.events.try_send(skipped(index)).unwrap();
    }
    let (end_tx, end) = oneshot::channel();
    end_tx.send(()).unwrap();

    spawn_session(
        &Handle::current(),
        4,
        std::future::ready(()),
        h.channels,
        run_until(h.events, h.stats, end),
        test_support::sink(async { anyhow::bail!("the archive could not be read") }),
        h.port,
    );
    let (deliveries, done) = collect(&mut h.recorded).await;

    let delivered: usize = deliveries.iter().map(|d| d.events.len()).sum();
    assert_eq!(delivered, 3);
    assert!(
        matches!(&done.finish, Err(text) if text.contains("could not be read")),
        "{:?}",
        done.finish
    );
    assert!(h.recorded.try_recv().is_err(), "nothing may follow done");
}

#[tokio::test]
async fn a_panicking_run_future_reports_the_panic_as_the_engine_error() {
    let mut h = harness(4);
    drop((h.events, h.stats));

    spawn_session(
        &Handle::current(),
        2,
        std::future::ready(()),
        h.channels,
        async { panic!("engine bug") },
        test_support::sink(finished_ok()),
        h.port,
    );
    let (_, done) = collect(&mut h.recorded).await;

    assert!(
        matches!(&done.engine, Err(text) if text.contains("panicked")),
        "{:?}",
        done.engine
    );
}

#[tokio::test]
async fn a_start_failure_reports_the_error_text() {
    let mut h = harness(4);
    drop((h.events, h.stats));

    spawn_session(
        &Handle::current(),
        2,
        std::future::ready(()),
        h.channels,
        async { Err(anyhow::anyhow!("connect refused").context("Failed to connect")) },
        test_support::sink(finished_ok()),
        h.port,
    );
    let (_, done) = collect(&mut h.recorded).await;

    assert_eq!(
        done.engine.unwrap_err(),
        "Failed to connect: connect refused"
    );
}

#[tokio::test]
async fn a_dropped_ack_sender_ends_the_forwarder() {
    let mut h = harness(4);
    let (_end_tx, end) = oneshot::channel();
    let events = h.events.clone();
    spawn_session(
        &Handle::current(),
        1,
        std::future::ready(()),
        h.channels,
        run_until(h.events, h.stats, end),
        test_support::sink(finished_ok()),
        h.port,
    );
    events.send(skipped(0)).await.unwrap();

    let (_, ack) = next_delivery(&mut h.recorded).await;
    drop(ack);
    let closed = tokio::time::timeout(Duration::from_secs(5), events.closed()).await;

    assert!(closed.is_ok(), "the forwarder kept the event channel open");
}

#[tokio::test]
async fn a_dead_statistics_watch_still_lets_events_through() {
    let mut h = harness(4);
    drop(h.stats);
    h.events.try_send(skipped(0)).unwrap();
    let (end_tx, end) = oneshot::channel();
    let (unused_stats, _) = watch::channel(snapshot());
    end_tx.send(()).unwrap();

    spawn_session(
        &Handle::current(),
        1,
        std::future::ready(()),
        h.channels,
        run_until(h.events, unused_stats, end),
        test_support::sink(finished_ok()),
        h.port,
    );
    let (deliveries, _) = collect(&mut h.recorded).await;

    assert_eq!(deliveries.len(), 1);
    assert!(deliveries[0].stats.is_none());
}

#[tokio::test]
async fn every_delivery_and_the_done_carry_the_session_id() {
    let mut h = harness(16);
    h.events.try_send(skipped(0)).unwrap();
    h.stats.send_modify(|stats| stats.received = 1);
    let (end_tx, end) = oneshot::channel();
    end_tx.send(()).unwrap();

    spawn_session(
        &Handle::current(),
        42,
        std::future::ready(()),
        h.channels,
        run_until(h.events, h.stats, end),
        test_support::sink(finished_ok()),
        h.port,
    );
    let (deliveries, done) = collect(&mut h.recorded).await;

    assert!(!deliveries.is_empty());
    assert!(deliveries.iter().all(|d| d.session == 42));
    assert_eq!(done.session, 42);
}

#[tokio::test]
async fn stop_sends_the_stop_command() {
    let mut h = harness(4);
    let (_end_tx, end) = oneshot::channel();

    let session = spawn_session(
        &Handle::current(),
        3,
        std::future::ready(()),
        h.channels,
        run_until(h.events, h.stats, end),
        test_support::sink(finished_ok()),
        h.port,
    );
    session.stop();

    assert!(matches!(h.commands.recv().await, Some(EngineCommand::Stop)));
    assert_eq!(session.id(), 3);
}

#[tokio::test]
async fn nothing_reaches_the_port_before_the_start_signal() {
    let mut h = harness(4);
    h.events.try_send(skipped(0)).unwrap();
    let (end_tx, end) = oneshot::channel();
    end_tx.send(()).unwrap();
    let (start, started) = oneshot::channel::<()>();

    spawn_session(
        &Handle::current(),
        5,
        async move { drop(started.await) },
        h.channels,
        run_until(h.events, h.stats, end),
        test_support::sink(finished_ok()),
        h.port,
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    let early = h.recorded.try_recv().is_ok();
    start.send(()).unwrap();
    let (deliveries, _) = collect(&mut h.recorded).await;

    assert!(!early, "the port heard from the session before its start");
    assert_eq!(deliveries.len(), 1);
}

#[tokio::test]
async fn a_panicking_at_end_pass_still_reports_the_end() {
    let mut h = harness(4);
    drop((h.events, h.stats));

    spawn_session(
        &Handle::current(),
        6,
        std::future::ready(()),
        h.channels,
        async { Ok(summary(RunEnd::Stopped)) },
        test_support::sink(async { panic!("at-end bug") }),
        h.port,
    );
    let (_, done) = collect(&mut h.recorded).await;

    assert!(
        matches!(&done.finish, Err(text) if text.contains("panicked")),
        "{:?}",
        done.finish
    );
}

#[tokio::test]
async fn a_panicking_forwarder_fails_a_clean_run() {
    struct PanickingPort(mpsc::UnboundedSender<EngineDone>);
    impl UiPort for PanickingPort {
        fn deliver(&self, _delivery: UiDelivery, _ack: oneshot::Sender<()>) {
            panic!("UI port bug");
        }
        fn done(&self, done: EngineDone) {
            self.0.send(done).unwrap();
        }
    }
    let h = harness(4);
    h.events.try_send(skipped(0)).unwrap();
    let (ends, mut ended) = mpsc::unbounded_channel();
    let (end_tx, end) = oneshot::channel();
    end_tx.send(()).unwrap();

    spawn_session(
        &Handle::current(),
        8,
        std::future::ready(()),
        h.channels,
        run_until(h.events, h.stats, end),
        test_support::sink(finished_ok()),
        Arc::new(PanickingPort(ends)),
    );
    let done = ended.recv().await.unwrap();

    assert!(
        matches!(&done.engine, Err(text) if text.contains("forwarder")),
        "{:?}",
        done.engine
    );
}

#[tokio::test]
async fn stop_on_a_full_command_channel_is_handed_to_the_runtime() {
    let (session, mut commands) = test_support::detached_session(1, &Handle::current());
    for _ in 0..4 {
        session.stop();
    }

    session.stop();
    let mut received = 0;
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(200), commands.recv()).await
    {
        received += 1;
    }

    assert_eq!(received, 5);
}

fn contains(word: &str) -> CaptureFilters {
    CaptureFilters {
        time: None,
        text: Some(eh_core::filters::TextFilter::Contains(vec![
            word.to_owned(),
        ])),
    }
}

#[tokio::test]
async fn update_filters_sets_the_sink_filters_and_sends_one_command() {
    let (session, mut commands, calls) = test_support::recorded_session(1, &Handle::current());

    session.update_filters(contains("bacon"));

    assert_eq!(*calls.lock().unwrap(), vec![contains("bacon")]);
    assert!(matches!(
        commands.recv().await,
        Some(EngineCommand::UpdateFilters(filters)) if filters == contains("bacon")
    ));
    assert!(commands.try_recv().is_err());
}

#[tokio::test]
async fn update_filters_on_an_ended_session_still_updates_the_sink() {
    let (session, commands, calls) = test_support::recorded_session(1, &Handle::current());
    drop(commands);

    session.update_filters(contains("eggs"));

    assert_eq!(*calls.lock().unwrap(), vec![contains("eggs")]);
}
