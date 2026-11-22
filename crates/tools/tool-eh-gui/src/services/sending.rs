//! The send worker: resolve the target, open a producer, send, close, and report every step,
//! with a bounded stop after a cancel. The work runs against a [`SendPort`], so tests drive
//! every exit without an event hub.

use super::{apply_from_runtime, resolve_target};
use crate::archive::bulk::progress_due;
use crate::send::compose::BODY_CAP;
use crate::send::run::Names;
use crate::send::template::take_sequence;
use crate::send::{SendInput, SendRequest};
use crate::services::ConnectRequest;
use crate::state::{AppState, Input, SEND_STOP_BOUND};
use chrono::Utc;
use rune::AppHandle;
use shared_eventhub_core::cancel::{CANCEL_POLL, flag_set};
use shared_eventhub_core::connection::EventHubTarget;
use shared_eventhub_core::producer::{
    EventHubProducer, OutgoingEvent, SendEnd, SendPlan, SendProgress, SendSummary, next_event,
    open_producer,
};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::time::{Instant, MissedTickBehavior};
use tracing::{debug, warn};

/// The text of a run whose worker panicked.
pub const STOPPED_UNEXPECTEDLY: &str = "the send stopped unexpectedly";
/// The text of a run that did not stop within [`SEND_STOP_BOUND`] after a cancel.
pub const DID_NOT_STOP: &str =
    "the send did not stop in time; the last batch may or may not have been stored";
/// The close error of a producer whose close outlived its bound.
pub const CLOSE_TOO_SLOW: &str = "it did not finish in time";

/// An open that a cancel can abandon and a cleanup task can finish.
pub type OpenFuture<P> = Pin<Box<dyn Future<Output = anyhow::Result<P>> + Send>>;

/// What the worker needs from the outside world, one method per step.
pub trait SendPort: Send + Sync + 'static {
    type Producer: Send + Sync + 'static;

    /// The target the request's form describes, reading the secrets file when it must.
    fn resolve(
        &self,
        request: &ConnectRequest,
    ) -> impl Future<Output = anyhow::Result<EventHubTarget>> + Send;

    /// Opens a producer for `target`.
    fn open(&self, target: EventHubTarget) -> OpenFuture<Self::Producer>;

    /// The names the producer resolved.
    fn names(producer: &Self::Producer) -> Names;

    /// Sends the events `plan` counts, asking `next_event` for each one by its zero-based index,
    /// and calls `progress` after each batch.
    fn send<'a>(
        &'a self,
        producer: &'a Self::Producer,
        plan: &'a SendPlan,
        next_event: impl FnMut(u64) -> anyhow::Result<OutgoingEvent> + Send + 'a,
        cancel: &'a AtomicBool,
        progress: impl FnMut(&SendProgress) + Send + 'a,
    ) -> impl Future<Output = SendSummary> + Send + 'a;

    /// Closes the producer.
    fn close(&self, producer: Self::Producer) -> impl Future<Output = anyhow::Result<()>> + Send;
}

/// The production port: core's producer over the secrets gate.
pub struct HubPort;

impl SendPort for HubPort {
    type Producer = EventHubProducer;

    async fn resolve(&self, request: &ConnectRequest) -> anyhow::Result<EventHubTarget> {
        resolve_target(request).await
    }

    fn open(&self, target: EventHubTarget) -> OpenFuture<EventHubProducer> {
        Box::pin(async move { open_producer(&target).await })
    }

    fn names(producer: &EventHubProducer) -> Names {
        Names {
            namespace: producer.fully_qualified_namespace.clone(),
            event_hub: producer.event_hub_name.clone(),
        }
    }

    fn send<'a>(
        &'a self,
        producer: &'a EventHubProducer,
        plan: &'a SendPlan,
        next_event: impl FnMut(u64) -> anyhow::Result<OutgoingEvent> + Send + 'a,
        cancel: &'a AtomicBool,
        progress: impl FnMut(&SendProgress) + Send + 'a,
    ) -> impl Future<Output = SendSummary> + Send + 'a {
        producer.send(plan, next_event, cancel, progress)
    }

    async fn close(&self, producer: EventHubProducer) -> anyhow::Result<()> {
        producer.close().await
    }
}

/// Runs `request` on the production port, posting every result to the app.
pub(crate) fn spawn_send(rt: &Handle, handle: AppHandle<AppState>, request: SendRequest) {
    let post = move |input: SendInput| apply_from_runtime(&handle, Input::Send(input));
    spawn_joined(rt, Arc::new(HubPort), request, post);
}

/// Runs the worker in its own task and awaits it in another, so a panic in the worker still
/// ends the run with a [`SendInput::Finished`].
pub fn spawn_joined<P: SendPort>(
    rt: &Handle,
    port: Arc<P>,
    request: SendRequest,
    post: impl Fn(SendInput) + Send + Sync + 'static,
) {
    let post = Arc::new(post);
    let run = request.run;
    let last_sent = Arc::new(AtomicU64::new(0));
    let started = Instant::now();
    let worker = rt.spawn(run_send(
        port,
        request,
        Arc::clone(&post),
        Arc::clone(&last_sent),
    ));
    rt.spawn(async move {
        if let Err(error) = worker.await {
            warn!(run, error = %error, "the send worker stopped unexpectedly");
            post(SendInput::Finished {
                run,
                summary: failed_summary(
                    last_sent.load(Ordering::Acquire),
                    started.elapsed(),
                    STOPPED_UNEXPECTEDLY,
                ),
                close_error: None,
            });
        }
    });
}

fn failed_summary(sent: u64, elapsed: Duration, reason: &str) -> SendSummary {
    SendSummary {
        sent,
        batches: 0,
        elapsed,
        end: SendEnd::Failed(reason.to_owned()),
    }
}

fn cancelled_before_sending(elapsed: Duration) -> SendSummary {
    SendSummary {
        sent: 0,
        batches: 0,
        elapsed,
        end: SendEnd::Cancelled,
    }
}

/// One run: resolve, open (raced against a cancel), send, close, report. Posts at least one
/// [`SendInput::Finished`] on every path that returns.
pub async fn run_send<P: SendPort>(
    port: Arc<P>,
    request: SendRequest,
    post: Arc<impl Fn(SendInput) + Send + Sync + 'static>,
    last_sent: Arc<AtomicU64>,
) {
    let SendRequest {
        run,
        target,
        event,
        template,
        plan,
        cancel,
        sequence,
    } = request;
    let started = Instant::now();
    let finish = |summary: SendSummary, close_error: Option<String>| {
        post(SendInput::Finished {
            run,
            summary,
            close_error,
        });
    };

    let target = match port.resolve(&target).await {
        Ok(target) => target,
        Err(error) => {
            finish(
                failed_summary(0, started.elapsed(), &format!("{error:#}")),
                None,
            );
            return;
        }
    };
    if cancel.load(Ordering::Acquire) {
        finish(cancelled_before_sending(started.elapsed()), None);
        return;
    }
    let Some(producer) = open_unless_cancelled(&port, target, &cancel, run).await else {
        finish(cancelled_before_sending(started.elapsed()), None);
        return;
    };
    let producer = match producer {
        Ok(producer) => producer,
        Err(error) => {
            finish(
                failed_summary(0, started.elapsed(), &format!("{error:#}")),
                None,
            );
            return;
        }
    };
    post(SendInput::Started {
        run,
        target: P::names(&producer),
        at: Utc::now(),
    });

    let mut last_post = None;
    let progress = |progress: &SendProgress| {
        last_sent.store(progress.sent, Ordering::Release);
        let now = Instant::now().into_std();
        if progress_due(last_post, now) {
            last_post = Some(now);
            post(SendInput::Progress {
                run,
                sent: progress.sent,
            });
        }
    };
    let template = template.as_deref();
    let next = |_: u64| {
        next_event(
            &event,
            template,
            BODY_CAP,
            Utc::now(),
            take_sequence(template, &sequence),
        )
    };
    let send = port.send(&producer, &plan, next, &cancel, progress);
    let (summary, stop_by) = bounded_after_cancel(send, &cancel, || {
        warn!(run, "the send did not stop in time after its cancel");
        finish(
            failed_summary(
                last_sent.load(Ordering::Acquire),
                started.elapsed(),
                DID_NOT_STOP,
            ),
            None,
        );
    })
    .await;

    // After a cancel the close shares the stop deadline with the batch in flight; once that
    // deadline has passed, the close still gets a bound of its own.
    let now = Instant::now();
    let close_by = match stop_by {
        Some(deadline) if deadline > now => deadline,
        Some(_) | None => now + SEND_STOP_BOUND,
    };
    let close_error = match tokio::time::timeout_at(close_by, port.close(producer)).await {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(format!("{error:#}")),
        Err(_) => Some(CLOSE_TOO_SLOW.to_owned()),
    };
    finish(summary, close_error);
}

/// Opens a producer for `target` unless `cancel` is set first. An open that finished wins a
/// tie. A cancelled open is handed to a cleanup task that closes the producer if it opens.
async fn open_unless_cancelled<P: SendPort>(
    port: &Arc<P>,
    target: EventHubTarget,
    cancel: &AtomicBool,
    run: u64,
) -> Option<anyhow::Result<P::Producer>> {
    let mut open = port.open(target);
    tokio::select! {
        biased;
        opened = &mut open => Some(opened),
        () = flag_set(cancel) => {
            let port = Arc::clone(port);
            tokio::spawn(async move {
                match open.await {
                    Ok(producer) => {
                        debug!(run, "closing a producer that opened after its send was cancelled");
                        if let Err(error) = port.close(producer).await {
                            // Best-effort cleanup: the run already ended cancelled.
                            warn!(run, error = %format!("{error:#}"), "closing an abandoned producer failed");
                        }
                    }
                    Err(error) => {
                        debug!(run, error = %format!("{error:#}"), "an abandoned open failed");
                    }
                }
            });
            None
        }
    }
}

/// Awaits `send`. Once `cancel` is seen set, the run has [`SEND_STOP_BOUND`] to stop; past it,
/// `overdue` runs once and the send is still awaited to its end. Returns the summary and the
/// stop deadline, when a cancel set one.
async fn bounded_after_cancel(
    send: impl Future<Output = SendSummary>,
    cancel: &AtomicBool,
    overdue: impl FnOnce(),
) -> (SendSummary, Option<Instant>) {
    let mut send = std::pin::pin!(send);
    // A tick rather than `flag_set`: the wait goes on after the flag flips, and `flag_set`
    // would then resolve on every iteration.
    let mut tick = tokio::time::interval(CANCEL_POLL);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut deadline = None;
    let mut overdue = Some(overdue);
    loop {
        tokio::select! {
            biased;
            summary = &mut send => return (summary, deadline),
            _ = tick.tick() => {
                if cancel.load(Ordering::Acquire) {
                    let deadline = *deadline.get_or_insert_with(|| Instant::now() + SEND_STOP_BOUND);
                    if Instant::now() >= deadline
                        && let Some(overdue) = overdue.take()
                    {
                        overdue();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
