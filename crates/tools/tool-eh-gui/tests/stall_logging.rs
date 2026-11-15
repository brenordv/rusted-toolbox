//! A UI that holds a delivery's ack past two seconds gets one stall warning, and one info line
//! when it accepts again. This is the only test in its binary, so a global subscriber sees every
//! event without other tests' events mixed in.

use chrono::Utc;
use eh_gui::bridge::{EngineDone, SessionSink, UiDelivery, UiPort, spawn_session};
use shared_eventhub_core::engine::{EngineChannels, EngineEvent, RunEnd, RunSummary, SkipReason};
use shared_eventhub_core::stats::StatsTracker;
use shared_eventhub_persistence::export::ExportOutcome;
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot, watch};

#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct AckChannel(
    mpsc::UnboundedSender<oneshot::Sender<()>>,
    mpsc::UnboundedSender<()>,
);

impl UiPort for AckChannel {
    fn deliver(&self, _delivery: UiDelivery, ack: oneshot::Sender<()>) {
        self.0.send(ack).unwrap();
    }

    fn done(&self, _done: EngineDone) {
        self.1.send(()).unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn a_stalled_ui_is_warned_once_and_its_recovery_logged() {
    let buffer = SharedBuffer::default();
    let writer = buffer.clone();
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish(),
    )
    .unwrap();
    let (events, events_rx) = mpsc::channel(4);
    let (stats, stats_rx) = watch::channel(StatsTracker::new(Utc::now()).snapshot());
    let (commands, _commands_rx) = mpsc::channel(1);
    let (acks_tx, mut acks) = mpsc::unbounded_channel();
    let (done_tx, mut done) = mpsc::unbounded_channel();
    let (end_tx, end) = oneshot::channel::<()>();
    let sender = events.clone();
    spawn_session(
        &Handle::current(),
        9,
        std::future::ready(()),
        EngineChannels {
            commands,
            events: events_rx,
            stats: stats_rx,
        },
        async move {
            drop(end.await);
            drop((events, stats));
            Ok(RunSummary {
                stats: StatsTracker::new(Utc::now()).snapshot(),
                partitions: BTreeMap::new(),
                end: RunEnd::Stopped,
            })
        },
        SessionSink {
            finish: async { Ok(ExportOutcome::default()) },
            set_filters: Arc::new(|_| {}),
        },
        Arc::new(AckChannel(acks_tx, done_tx)),
    );
    sender
        .send(EngineEvent::Skipped {
            partition_id: "0".to_owned(),
            reason: SkipReason::Filtered,
        })
        .await
        .unwrap();

    let held = acks.recv().await.unwrap();
    tokio::time::sleep(Duration::from_secs(5)).await;
    held.send(()).unwrap();
    drop(sender);
    end_tx.send(()).unwrap();
    while let Some(ack) = tokio::select! {
        ack = acks.recv() => ack,
        _ = done.recv() => None,
    } {
        ack.send(()).unwrap();
    }

    let logs = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
    assert_eq!(
        logs.matches("UI has not accepted a delivery for 2s")
            .count(),
        1,
        "{logs}"
    );
    assert_eq!(
        logs.matches("the UI accepts deliveries again").count(),
        1,
        "{logs}"
    );
}
