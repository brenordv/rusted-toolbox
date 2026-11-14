use super::*;
use shared_eventhub_core::engine::{PartitionEnd, RunEnd};
use shared_eventhub_core::stats::StatsSnapshot;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use tokio::sync::{mpsc, oneshot, watch};

type TestRenderer = Renderer<Vec<u8>, Vec<u8>>;

struct Harness {
    channels: EngineChannels,
    commands: mpsc::Receiver<EngineCommand>,
    events: mpsc::Sender<EngineEvent>,
    stats: watch::Sender<StatsSnapshot>,
}

fn harness() -> Harness {
    let (commands_tx, commands_rx) = mpsc::channel(16);
    let (events_tx, events_rx) = mpsc::channel(64);
    let (stats_tx, stats_rx) = watch::channel(StatsSnapshot::default());
    Harness {
        channels: EngineChannels {
            commands: commands_tx,
            events: events_rx,
            stats: stats_rx,
        },
        commands: commands_rx,
        events: events_tx,
        stats: stats_tx,
    }
}

fn renderer() -> TestRenderer {
    Renderer::new(Vec::new(), Vec::new(), false, 200)
}

fn summary(end: RunEnd) -> RunSummary {
    RunSummary {
        stats: StatsSnapshot::default(),
        partitions: BTreeMap::from([("0".to_owned(), PartitionEnd::Stopped)]),
        end,
    }
}

fn started() -> EngineEvent {
    EngineEvent::Started {
        partitions: vec!["0".to_owned(), "1".to_owned()],
    }
}

fn stderr_of(renderer: TestRenderer) -> String {
    String::from_utf8(renderer.into_parts().1).unwrap()
}

/// A run future that resolves `Stopped` once it receives a `Stop`, then
/// keeps counting any further commands for a while.
async fn stops_on_command(
    mut commands: mpsc::Receiver<EngineCommand>,
    received: Arc<AtomicUsize>,
) -> anyhow::Result<RunSummary> {
    while let Some(command) = commands.recv().await {
        if matches!(command, EngineCommand::Stop) {
            received.fetch_add(1, Ordering::SeqCst);
            break;
        }
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    while let Ok(command) = commands.try_recv() {
        if matches!(command, EngineCommand::Stop) {
            received.fetch_add(1, Ordering::SeqCst);
        }
    }
    Ok(summary(RunEnd::Stopped))
}

#[tokio::test(start_paused = true)]
async fn a_completed_run_returns_its_summary() {
    let h = harness();
    let mut renderer = renderer();
    let shutdown = AtomicBool::new(false);

    let ended = run(
        h.channels,
        async { Ok(summary(RunEnd::Completed)) },
        &shutdown,
        &mut renderer,
    )
    .await;

    assert!(matches!(
        ended,
        LoopEnd::Finished(Ok(RunSummary {
            end: RunEnd::Completed,
            ..
        }))
    ));
}

#[tokio::test(start_paused = true)]
async fn a_run_that_cannot_start_returns_its_error() {
    let h = harness();
    let mut renderer = renderer();
    let shutdown = AtomicBool::new(false);

    let ended = run(
        h.channels,
        async { Err(anyhow::anyhow!("namespace not allowed")) },
        &shutdown,
        &mut renderer,
    )
    .await;

    assert!(
        matches!(&ended, LoopEnd::Finished(Err(error)) if error.to_string() == "namespace not allowed")
    );
}

#[tokio::test(start_paused = true)]
async fn ctrl_c_before_started_drops_the_run_and_says_nothing_was_captured() {
    let h = harness();
    let mut renderer = renderer();
    let shutdown = AtomicBool::new(true);
    let (alive, dropped) = oneshot::channel::<()>();
    let connecting = async move {
        let _alive = alive;
        std::future::pending::<anyhow::Result<RunSummary>>().await
    };

    let ended = run(h.channels, connecting, &shutdown, &mut renderer).await;

    assert!(matches!(ended, LoopEnd::InterruptedBeforeStart));
    assert!(dropped.await.is_err(), "the run future must be dropped");
    assert!(stderr_of(renderer).contains("Interrupted while connecting; nothing was captured."),);
}

#[tokio::test(start_paused = true)]
async fn ctrl_c_with_started_already_queued_stops_instead_of_dropping() {
    let h = harness();
    let mut renderer = renderer();
    let shutdown = AtomicBool::new(true);
    let received = Arc::new(AtomicUsize::new(0));
    h.events.send(started()).await.unwrap();

    let ended = run(
        h.channels,
        stops_on_command(h.commands, Arc::clone(&received)),
        &shutdown,
        &mut renderer,
    )
    .await;

    assert!(matches!(
        ended,
        LoopEnd::Finished(Ok(RunSummary {
            end: RunEnd::Stopped,
            ..
        }))
    ));
    assert_eq!(received.load(Ordering::SeqCst), 1);
    assert!(stderr_of(renderer).contains("Reading partitions: 0, 1"));
}

#[tokio::test(start_paused = true)]
async fn ctrl_c_after_started_sends_exactly_one_stop() {
    let h = harness();
    let mut renderer = renderer();
    let shutdown = Arc::new(AtomicBool::new(false));
    let received = Arc::new(AtomicUsize::new(0));
    let events = h.events.clone();
    let flag = Arc::clone(&shutdown);
    tokio::spawn(async move {
        events.send(started()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(350)).await;
        flag.store(true, Ordering::SeqCst);
    });

    let ended = run(
        h.channels,
        stops_on_command(h.commands, Arc::clone(&received)),
        &shutdown,
        &mut renderer,
    )
    .await;

    assert!(matches!(ended, LoopEnd::Finished(Ok(_))));
    assert_eq!(received.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_stop_sent_after_the_engine_stopped_listening_still_ends_with_the_run() {
    let h = harness();
    let mut renderer = renderer();
    let shutdown = AtomicBool::new(true);
    h.events.send(started()).await.unwrap();
    let commands = h.commands;
    let ending = async move {
        drop(commands);
        tokio::time::sleep(Duration::from_millis(500)).await;
        Ok(summary(RunEnd::Completed))
    };

    let ended = run(h.channels, ending, &shutdown, &mut renderer).await;

    assert!(matches!(
        ended,
        LoopEnd::Finished(Ok(RunSummary {
            end: RunEnd::Completed,
            ..
        }))
    ));
}

#[tokio::test(start_paused = true)]
async fn a_partition_failure_is_shown_once() {
    let h = harness();
    let mut renderer = renderer();
    let shutdown = AtomicBool::new(false);
    h.events.send(started()).await.unwrap();
    h.events
        .send(EngineEvent::Error {
            partition_id: Some("1".to_owned()),
            error: "recreated hub".to_owned(),
            fatal: false,
        })
        .await
        .unwrap();
    h.events
        .send(EngineEvent::PartitionEnded {
            partition_id: "1".to_owned(),
            end: PartitionEnd::Failed("recreated hub".to_owned()),
        })
        .await
        .unwrap();

    run(
        h.channels,
        async { Ok(summary(RunEnd::Stopped)) },
        &shutdown,
        &mut renderer,
    )
    .await;

    let stderr = stderr_of(renderer);
    assert_eq!(stderr.matches("recreated hub").count(), 1, "{stderr}");
    assert!(
        stderr.contains("Partition 1 failed: recreated hub"),
        "{stderr}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_stopped_partition_end_is_not_shown() {
    let h = harness();
    let mut renderer = renderer();
    let shutdown = AtomicBool::new(false);
    h.events
        .send(EngineEvent::PartitionEnded {
            partition_id: "0".to_owned(),
            end: PartitionEnd::Stopped,
        })
        .await
        .unwrap();

    run(
        h.channels,
        async { Ok(summary(RunEnd::Stopped)) },
        &shutdown,
        &mut renderer,
    )
    .await;

    assert!(!stderr_of(renderer).contains("Partition 0"));
}

#[tokio::test(start_paused = true)]
async fn statistics_redraw_the_progress_line_and_it_ends_with_a_newline() {
    let h = harness();
    let mut renderer = renderer();
    let shutdown = AtomicBool::new(false);
    let stats = h.stats;
    let run_future = async move {
        stats.send_replace(StatsSnapshot {
            received: 42,
            ..StatsSnapshot::default()
        });
        tokio::time::sleep(Duration::from_millis(250)).await;
        Ok(summary(RunEnd::Completed))
    };

    run(h.channels, run_future, &shutdown, &mut renderer).await;

    let stdout = String::from_utf8(renderer.into_parts().0).unwrap();
    assert!(stdout.contains("Received: 42"), "{stdout}");
    assert!(stdout.ends_with('\n'), "{stdout:?}");
}
