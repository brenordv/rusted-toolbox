//! `eh send`: publishes one body a given number of times.

use crate::models::{RunOutcome, SendJob};
use crate::progress::{ExportProgressLine, Renderer, connecting_notice, display, format_elapsed};
use crate::send_cli::{RENDER_CAP, template_notices};
use crate::target_setup::{TargetSetup, prepare_target};
use anyhow::anyhow;
use chrono::Utc;
use eh_core::cancel::{CANCEL_POLL, flag_set};
use eh_core::producer::{
    EventHubProducer, SendEnd, SendPlan, SendProgress, SendSummary, next_event, open_producer,
};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::MissedTickBehavior;

const STOPPING: &str =
    "Stopping after the batch in flight (the SDK can retry a batch for about a minute)...";

/// Sends the job's event: resolves the target (saving the config first when
/// asked), opens a producer (Ctrl-C while connecting ends the run), names
/// the destination, sends with a progress line, closes the producer, and
/// prints the summary.
///
/// # Errors
/// Fails when the target cannot be resolved or saved, the producer cannot be
/// opened, the run failed, or the producer fails to close.
pub async fn run(job: SendJob, shutdown: &AtomicBool) -> anyhow::Result<RunOutcome> {
    let mut renderer = Renderer::terminal(false);
    let target = prepare_target(
        &TargetSetup {
            config: &job.config,
            given: &job.secrets,
            paths: &job.paths,
            save_config: job.save_config,
            home: &job.home,
        },
        &mut renderer,
    )?;

    for notice in job
        .template
        .as_deref()
        .map(template_notices)
        .unwrap_or_default()
    {
        renderer.notice(&notice);
    }
    renderer.notice(&connecting_notice(&job.config.target, None));
    let Some(producer) = open_or_interrupt(open_producer(&target), shutdown).await? else {
        renderer.notice("Interrupted while connecting; nothing was sent.");
        return Ok(RunOutcome::Interrupted);
    };
    renderer.notice(&destination_notice(&producer, &job.plan));

    let summary = send_with_progress(&producer, &job, shutdown, &mut renderer).await;
    let closed = producer.close().await;
    renderer.block(&summary_block(
        &summary,
        job.plan.count(),
        closed.as_ref().err(),
    ));

    send_outcome(summary.end, closed)
}

/// Awaits `open` unless Ctrl-C comes first, which drops it. An open that
/// finished wins a tie, so every producer that was opened is returned (and
/// later closed).
async fn open_or_interrupt<T>(
    open: impl Future<Output = anyhow::Result<T>>,
    shutdown: &AtomicBool,
) -> anyhow::Result<Option<T>> {
    tokio::select! {
        biased;
        opened = open => opened.map(Some),
        () = flag_set(shutdown) => Ok(None),
    }
}

/// Runs the send with the progress line, printing the stop notice once when
/// a stop is still waiting on a batch in flight.
async fn send_with_progress<O: Write, E: Write>(
    producer: &EventHubProducer,
    job: &SendJob,
    shutdown: &AtomicBool,
    renderer: &mut Renderer<O, E>,
) -> SendSummary {
    let requested = job.plan.count();
    let mut line = ExportProgressLine::stdout();
    let summary = {
        let send = producer.send(
            &job.plan,
            |index| {
                next_event(
                    &job.event,
                    job.template.as_deref(),
                    RENDER_CAP,
                    Utc::now(),
                    index,
                )
            },
            shutdown,
            |progress| line.draw_with(|_| send_line(progress, requested)),
        );
        wait_noting_stop(send, shutdown, renderer).await
    };
    line.finish(true);
    summary
}

/// Awaits `send`, waking every [`CANCEL_POLL`]. On the second tick that sees
/// the flag set, the send has had longer than its own cancel poll to stop,
/// so it is waiting on a batch the SDK is still sending: say so once.
async fn wait_noting_stop<O: Write, E: Write>(
    send: impl Future<Output = SendSummary>,
    shutdown: &AtomicBool,
    renderer: &mut Renderer<O, E>,
) -> SendSummary {
    let mut send = std::pin::pin!(send);
    // A tick rather than `flag_set`: the wait goes on after the flag flips,
    // and `flag_set` would then resolve on every iteration.
    let mut tick = tokio::time::interval(CANCEL_POLL);
    // Delay, not the default burst: two ticks must stay a poll apart for the
    // second one to mean the send had time to stop.
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut ticks_since_stop = 0_u32;
    loop {
        tokio::select! {
            biased;
            summary = &mut send => return summary,
            _ = tick.tick() => {
                if shutdown.load(Ordering::Relaxed) {
                    ticks_since_stop += 1;
                    if ticks_since_stop == 2 {
                        renderer.notice(STOPPING);
                    }
                }
            }
        }
    }
}

/// The stderr line naming where the events go, from the names the producer
/// resolved, so a connection string or Key Vault target shows its hub too.
pub(crate) fn destination_notice(producer: &EventHubProducer, plan: &SendPlan) -> String {
    describe_destination(
        &producer.fully_qualified_namespace,
        &producer.event_hub_name,
        plan,
    )
}

fn describe_destination(namespace: &str, event_hub: &str, plan: &SendPlan) -> String {
    let routing = match plan.routing().key() {
        Some(key) => format!("partition key {}", display(key)),
        None => "automatic routing".to_owned(),
    };
    let pacing = match plan.pacing().get() {
        Some(interval) => format!(", one every {} ms", interval.as_millis()),
        None => String::new(),
    };
    format!(
        "Sending {} event(s) to {}/{}, {routing}{pacing}...",
        plan.count(),
        display(namespace),
        display(event_hub)
    )
}

/// The live send progress line.
pub(crate) fn send_line(progress: &SendProgress, requested: u64) -> String {
    format!(
        "Sent: {} of {requested} | Batches: {} | Rate: {:.2} ev/s | Runtime: {}",
        progress.sent,
        progress.batches,
        rate(progress.sent, progress.elapsed),
        format_elapsed(progress.elapsed)
    )
}

/// The block printed when a send ends.
pub(crate) fn summary_block(
    summary: &SendSummary,
    requested: u64,
    close_error: Option<&anyhow::Error>,
) -> String {
    let ended = match &summary.end {
        SendEnd::Completed => "completed".to_owned(),
        SendEnd::Cancelled => "stopped on request".to_owned(),
        SendEnd::Failed(reason) => format!("failed: {}", display(reason)),
    };
    let mut block = format!(
        "Send ended: {ended}\n\
         Sent: {} of {requested} | Batches: {} | Elapsed: {} | Average rate: {:.2} ev/s\n",
        summary.sent,
        summary.batches,
        format_elapsed(summary.elapsed),
        rate(summary.sent, summary.elapsed),
    );
    if let Some(error) = close_error {
        block.push_str(&format!(
            "Closing the connection failed: {}\n",
            display(format!("{error:#}"))
        ));
    }
    block
}

/// Maps how the send ended and the producer close to the exit outcome; a
/// failure wins over a stop. The error text is left unescaped: the
/// entrypoint escapes what it reports.
pub(crate) fn send_outcome(end: SendEnd, closed: anyhow::Result<()>) -> anyhow::Result<RunOutcome> {
    match (end, closed) {
        (SendEnd::Failed(reason), Ok(())) => Err(anyhow!("The send failed: {reason}")),
        (SendEnd::Failed(reason), Err(close)) => Err(anyhow!(
            "The send failed: {reason}; closing the connection also failed: {close:#}"
        )),
        (SendEnd::Completed | SendEnd::Cancelled, Err(close)) => Err(close.context(
            "Every event counted as sent was accepted, but closing the connection failed",
        )),
        (SendEnd::Completed, Ok(())) => Ok(RunOutcome::Completed),
        (SendEnd::Cancelled, Ok(())) => Ok(RunOutcome::Interrupted),
    }
}

fn rate(sent: u64, elapsed: Duration) -> f64 {
    let seconds = elapsed.as_secs_f64();
    if seconds > 0.0 {
        sent as f64 / seconds
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests;
