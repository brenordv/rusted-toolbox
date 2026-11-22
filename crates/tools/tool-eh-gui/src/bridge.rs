//! Carries one capture session's output from the engine to the UI.
//!
//! A session is a run task on the tokio runtime. It spawns a forwarder that batches engine
//! events and statistics into [`UiDelivery`] values, runs the engine, waits for the forwarder to
//! drain, runs the at-end pass, and reports [`EngineDone`] last. Outcomes come only from
//! `EngineDone`: the engine drops its event senders on every run end, so the event channel
//! closing says nothing about how the run ended.

use chrono::{DateTime, Utc};
use shared_eventhub_core::engine::{EngineChannels, EngineCommand, EngineEvent, RunSummary};
use shared_eventhub_core::filters::CaptureFilters;
use shared_eventhub_core::stats::StatsSnapshot;
use shared_eventhub_persistence::export::ExportOutcome;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Handle;
use tokio::sync::mpsc::error::{TryRecvError, TrySendError};
use tokio::sync::{mpsc, oneshot, watch};
use tracing::{debug, error, info, trace, warn};

/// The most events one delivery carries.
pub const BATCH_MAX: usize = 512;
/// How long the forwarder waits for the UI to accept a delivery before it logs a stall.
const STALL_WARN_AFTER: Duration = Duration::from_secs(2);

/// A batch of engine output for the UI, in engine order.
#[derive(Debug)]
pub struct UiDelivery {
    /// The session the batch belongs to; the UI ignores batches of any other session.
    pub session: u64,
    /// When the forwarder sent it.
    pub at: DateTime<Utc>,
    /// At most [`BATCH_MAX`] events.
    pub events: Vec<EngineEvent>,
    /// The latest statistics, when they changed since the previous delivery.
    pub stats: Option<StatsSnapshot>,
}

/// The end of a session: always the last message a session sends.
#[derive(Debug)]
pub struct EngineDone {
    pub session: u64,
    /// The run summary, or why the run produced none (a start failure or a panic).
    pub engine: Result<RunSummary, String>,
    /// The at-end pass: its counts, or why it could not read the archive.
    pub finish: Result<ExportOutcome, String>,
}

/// Where a session's output goes.
pub trait UiPort: Send + Sync + 'static {
    /// Hands `delivery` to the UI. The UI sends on `ack` once it applied the delivery; the
    /// forwarder sends nothing more until then.
    fn deliver(&self, delivery: UiDelivery, ack: oneshot::Sender<()>);

    /// Reports the end of the session.
    fn done(&self, done: EngineDone);
}

/// Replaces the filters a session's sink writes message files for.
pub type FilterHook = Arc<dyn Fn(CaptureFilters) + Send + Sync>;

/// The sink side of a session: the at-end pass (a lazy future) and the hook that keeps the
/// sink's message-file filters in step with the engine's.
pub struct SessionSink<F> {
    pub finish: F,
    pub set_filters: FilterHook,
}

/// The UI's handle on a running session: sends it commands without blocking.
pub struct EngineSession {
    id: u64,
    commands: mpsc::Sender<EngineCommand>,
    rt: Handle,
    set_filters: FilterHook,
}

impl std::fmt::Debug for EngineSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineSession")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl EngineSession {
    /// The session id its deliveries and its [`EngineDone`] carry.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// A session no engine runs, for the demo's seeded states: its commands go nowhere, and it
    /// never ends on its own.
    #[cfg(feature = "demo")]
    pub fn idle(id: u64, rt: &Handle) -> Self {
        let (commands, _) = mpsc::channel(1);
        Self {
            id,
            commands,
            rt: rt.clone(),
            set_filters: Arc::new(|_| {}),
        }
    }

    /// Asks the engine to stop every partition; the session then ends with its at-end pass.
    /// Safe to call more than once.
    pub fn stop(&self) {
        self.send(EngineCommand::Stop);
    }

    /// Replaces the capture filters: the sink's message files follow at once, the engine from
    /// the next event it processes.
    ///
    /// With the command channel full, the send moves to the runtime, so a later update that
    /// finds room can reach the engine first. The channel holds 16 commands and only a user's
    /// Apply sends filters, so that takes repeated presses while the engine stops reading.
    pub fn update_filters(&self, filters: CaptureFilters) {
        (self.set_filters)(filters.clone());
        self.send(EngineCommand::UpdateFilters(filters));
    }

    /// Sends `command` without ever blocking the calling thread: a full channel hands the send
    /// to the runtime instead. A blocking send from the UI thread could deadlock against a
    /// forwarder that waits for the UI's ack.
    fn send(&self, command: EngineCommand) {
        match self.commands.try_send(command) {
            Ok(()) => {}
            Err(TrySendError::Full(command)) => {
                let commands = self.commands.clone();
                let session = self.id;
                self.rt.spawn(async move {
                    if commands.send(command).await.is_err() {
                        debug!(
                            session,
                            "the engine ended before a queued command reached it"
                        );
                    }
                });
            }
            Err(TrySendError::Closed(_)) => {
                debug!(
                    session = self.id,
                    "the engine already ended; command not sent"
                );
            }
        }
    }
}

/// Starts a session on `rt` once `start` resolves: the run task drives `run`, forwards
/// `channels` to `port` under session id `id`, then awaits `sink.finish` (the at-end pass)
/// and reports [`EngineDone`]. Nothing reaches `port` before `start` resolves, so the owner
/// can hand the returned session to the UI first.
pub fn spawn_session<G, R, F>(
    rt: &Handle,
    id: u64,
    start: G,
    channels: EngineChannels,
    run: R,
    sink: SessionSink<F>,
    port: Arc<dyn UiPort>,
) -> EngineSession
where
    G: Future<Output = ()> + Send + 'static,
    R: Future<Output = anyhow::Result<RunSummary>> + Send + 'static,
    F: Future<Output = anyhow::Result<ExportOutcome>> + Send + 'static,
{
    let EngineChannels {
        commands,
        events,
        stats,
    } = channels;
    let SessionSink {
        finish,
        set_filters,
    } = sink;
    rt.spawn(async move {
        start.await;
        run_task(id, events, stats, run, finish, port).await;
    });
    EngineSession {
        id,
        commands,
        rt: rt.clone(),
        set_filters,
    }
}

async fn run_task<R, F>(
    session: u64,
    events: mpsc::Receiver<EngineEvent>,
    stats: watch::Receiver<StatsSnapshot>,
    run: R,
    finish: F,
    port: Arc<dyn UiPort>,
) where
    R: Future<Output = anyhow::Result<RunSummary>> + Send + 'static,
    F: Future<Output = anyhow::Result<ExportOutcome>> + Send + 'static,
{
    let forwarder = tokio::spawn(forward(session, events, stats, Arc::clone(&port)));
    let mut engine = match tokio::spawn(run).await {
        Ok(Ok(summary)) => Ok(summary),
        Ok(Err(error)) => Err(format!("{error:#}")),
        Err(error) => Err(format!("engine task panicked: {error}")),
    };

    // A dead forwarder lost deliveries the UI needed (possibly `Started`), so the session fails
    // even when the engine itself ended cleanly.
    if let Err(error) = forwarder.await {
        error!(session, error = %error, "the UI forwarder ended abnormally");
        let lost = format!("the UI forwarder ended abnormally: {error}");
        engine = match engine {
            Ok(_) => Err(lost),
            Err(text) => Err(format!("{text}; {lost}")),
        };
    }

    info!(session, "writing the at-end message files");
    let started = Instant::now();
    let finish = match tokio::spawn(finish).await {
        Ok(Ok(outcome)) => Ok(outcome),
        Ok(Err(error)) => Err(format!("{error:#}")),
        Err(error) => Err(format!("the at-end pass panicked: {error}")),
    };
    info!(
        session,
        elapsed_ms = started.elapsed().as_millis(),
        ok = finish.is_ok(),
        "the at-end pass ended"
    );

    port.done(EngineDone {
        session,
        engine,
        finish,
    });
}

/// Moves engine output to the UI, one delivery in flight at a time.
///
/// Waiting for each ack is the backpressure: while the UI is slow or hidden, events stay in the
/// engine's bounded channel and the engine drops view events and counts them, instead of this
/// task draining them into the UI's unbounded update queue. Ends when the event channel ends
/// (after delivering what was left) or when the UI drops an ack sender (the app is quitting).
async fn forward(
    session: u64,
    mut events: mpsc::Receiver<EngineEvent>,
    mut stats: watch::Receiver<StatsSnapshot>,
    port: Arc<dyn UiPort>,
) {
    let mut stats_alive = true;
    let mut drop_noted = false;

    loop {
        let mut batch = Vec::new();
        let mut ended = false;
        // `changed()` marks the value it reports as seen, so its arm carries the flag itself.
        let mut stats_changed = false;
        tokio::select! {
            event = events.recv() => match event {
                Some(event) => batch.push(event),
                None => ended = true,
            },
            changed = stats.changed(), if stats_alive => match changed {
                Ok(()) => stats_changed = true,
                Err(_) => stats_alive = false,
            }
        }
        if !ended {
            ended = drain_ready(&mut events, &mut batch);
        }

        let snapshot = {
            let latest = stats.borrow_and_update();
            (stats_changed || latest.has_changed()).then(|| latest.clone())
        };
        if batch.is_empty() && snapshot.is_none() {
            if ended {
                return;
            }
            continue;
        }

        if let Some(snapshot) = &snapshot
            && snapshot.ui_dropped > 0
            && !drop_noted
        {
            drop_noted = true;
            info!(
                session,
                ui_dropped = snapshot.ui_dropped,
                "the list skipped messages under load; the archive and counts are complete"
            );
        }

        trace!(session, events = batch.len(), "forwarding a delivery");
        let (ack, accepted) = oneshot::channel();
        port.deliver(
            UiDelivery {
                session,
                at: Utc::now(),
                events: batch,
                stats: snapshot,
            },
            ack,
        );
        if !await_ack(session, accepted).await || ended {
            return;
        }
    }
}

/// Moves what is already queued into `batch`, up to [`BATCH_MAX`]. Returns whether the channel
/// ended.
fn drain_ready(events: &mut mpsc::Receiver<EngineEvent>, batch: &mut Vec<EngineEvent>) -> bool {
    while batch.len() < BATCH_MAX {
        match events.try_recv() {
            Ok(event) => batch.push(event),
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => return true,
        }
    }
    false
}

/// Waits for the UI's ack, logging one warning per stall past [`STALL_WARN_AFTER`]. Returns
/// false when the UI dropped the ack sender.
async fn await_ack(session: u64, mut accepted: oneshot::Receiver<()>) -> bool {
    match tokio::time::timeout(STALL_WARN_AFTER, &mut accepted).await {
        Ok(Ok(())) => return true,
        Ok(Err(_)) => {
            debug!(
                session,
                "the UI dropped a delivery's ack; stopping the forwarder"
            );
            return false;
        }
        Err(_) => {}
    }

    warn!(
        session,
        "UI has not accepted a delivery for 2s; list updates paused, capture continues"
    );
    let stalled = Instant::now();
    match accepted.await {
        Ok(()) => {
            info!(
                session,
                stalled_ms = (stalled.elapsed() + STALL_WARN_AFTER).as_millis(),
                "the UI accepts deliveries again"
            );
            true
        }
        Err(_) => {
            debug!(
                session,
                "the UI dropped a delivery's ack; stopping the forwarder"
            );
            false
        }
    }
}

#[cfg(any(test, feature = "test-harness"))]
pub mod test_support {
    use super::*;
    use std::sync::Mutex;

    /// The filters a session's hook was called with, in order.
    pub type HookCalls = Arc<Mutex<Vec<CaptureFilters>>>;

    /// A session whose commands land in the returned receiver, for state tests.
    pub fn detached_session(
        id: u64,
        rt: &Handle,
    ) -> (EngineSession, mpsc::Receiver<EngineCommand>) {
        let (session, received, _) = recorded_session(id, rt);
        (session, received)
    }

    /// [`detached_session`] that also records every filter hook call.
    pub fn recorded_session(
        id: u64,
        rt: &Handle,
    ) -> (EngineSession, mpsc::Receiver<EngineCommand>, HookCalls) {
        let (commands, received) = mpsc::channel(4);
        let calls = HookCalls::default();
        let recorder = Arc::clone(&calls);
        let set_filters: FilterHook = Arc::new(move |filters| {
            recorder.lock().unwrap().push(filters);
        });
        (
            EngineSession {
                id,
                commands,
                rt: rt.clone(),
                set_filters,
            },
            received,
            calls,
        )
    }

    /// A sink side whose filter hook does nothing.
    pub fn sink<F>(finish: F) -> SessionSink<F> {
        SessionSink {
            finish,
            set_filters: Arc::new(|_| {}),
        }
    }
}

#[cfg(test)]
mod tests;
