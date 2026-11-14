//! The read run's event loop: renders engine events and statistics, and
//! turns Ctrl-C into a graceful stop.

use crate::progress::Renderer;
use shared_eventhub_core::engine::{EngineChannels, EngineCommand, EngineEvent, RunSummary};
use std::future::Future;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing::debug;

/// How often the loop wakes to look at the shutdown flag when nothing else
/// happens.
const SHUTDOWN_POLL: Duration = Duration::from_millis(100);

/// How the loop ended.
#[derive(Debug)]
pub(crate) enum LoopEnd {
    /// The run future resolved with this result.
    Finished(anyhow::Result<RunSummary>),
    /// Ctrl-C arrived before any partition started; the run future was
    /// dropped and nothing was captured.
    InterruptedBeforeStart,
}

/// Drives `run` to its end while rendering `channels`' events and
/// statistics.
///
/// When `shutdown` is set before the engine reported `Started`, the run is
/// still connecting (the SDK can retry for about a minute, and the engine
/// reads commands only once partitions run), so the run future is dropped.
/// That is safe only while no partition task exists: the engine queues
/// `Started` before it spawns any, and `run` is polled only here, so the
/// queue is drained first and an empty one proves there is none. Once
/// `Started` was seen, a single `Stop` is sent and the loop waits for the
/// run to end.
pub(crate) async fn run<F, O, E>(
    mut channels: EngineChannels,
    run: F,
    shutdown: &AtomicBool,
    renderer: &mut Renderer<O, E>,
) -> LoopEnd
where
    F: Future<Output = anyhow::Result<RunSummary>>,
    O: Write,
    E: Write,
{
    let mut run = std::pin::pin!(run);
    let mut started = false;
    let mut stop_sent = false;
    let mut poll = tokio::time::interval(SHUTDOWN_POLL);

    loop {
        if shutdown.load(Ordering::Relaxed) && !stop_sent {
            if !started {
                started = drain(&mut channels, renderer);
            }
            if !started {
                renderer.notice("Interrupted while connecting; nothing was captured.");
                renderer.finish_line();
                return LoopEnd::InterruptedBeforeStart;
            }
            stop_sent = true;
            if channels.commands.send(EngineCommand::Stop).await.is_err() {
                // The engine dropped its command receiver because the run is
                // already ending; the run future resolves next either way.
                debug!("The engine was already ending when the stop was sent");
            }
        }

        tokio::select! {
            biased;
            result = &mut run => {
                drain(&mut channels, renderer);
                renderer.finish_line();
                return LoopEnd::Finished(result);
            }
            Some(event) = channels.events.recv() => {
                started |= matches!(event, EngineEvent::Started { .. });
                renderer.engine_event(&event);
            }
            Ok(()) = channels.stats.changed() => {
                let snapshot = channels.stats.borrow_and_update().clone();
                renderer.progress(&snapshot);
            }
            _ = poll.tick() => {}
        }
    }
}

/// Renders every event already queued; returns whether one was `Started`.
fn drain<O: Write, E: Write>(channels: &mut EngineChannels, renderer: &mut Renderer<O, E>) -> bool {
    let mut started = false;
    while let Ok(event) = channels.events.try_recv() {
        started |= matches!(event, EngineEvent::Started { .. });
        renderer.engine_event(&event);
    }
    started
}

#[cfg(test)]
mod tests;
