//! The send loop over a [`Publisher`]: fills batches, paces them, and turns
//! every way a run can stop into a [`SendSummary`].

use super::{OutgoingEvent, PublishBatch, Publisher, SendEnd, SendPlan, SendProgress, SendSummary};
use crate::cancel::flag_set;
use anyhow::Context;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::Instant;
use tracing::{info, trace};

/// What the SDK allows for one event in a batch, in words for the error that
/// refuses a larger one. The SDK sizes a batch's first event twice, so a
/// single event can use only about half the publish limit.
const SINGLE_EVENT_LIMIT: &str = "the SDK limits a single event to about half the event hub's \
                                  publish limit (about 128 KB on Basic, 512 KB on Standard and \
                                  Premium)";

/// Counts that grow as batches are sent.
struct Counts {
    sent: u64,
    batches: u64,
}

/// Sends `plan` through `publisher`; see `EventHubProducer::send`.
pub(crate) async fn run<P: Publisher>(
    publisher: &P,
    plan: &SendPlan,
    mut next_event: impl FnMut(u64) -> anyhow::Result<OutgoingEvent> + Send,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&SendProgress) + Send,
) -> SendSummary {
    let started = Instant::now();
    let mut counts = Counts {
        sent: 0,
        batches: 0,
    };
    let end = send_all(
        publisher,
        plan,
        &mut next_event,
        cancel,
        &mut progress,
        &mut counts,
        started,
    )
    .await;
    let elapsed = started.elapsed();
    info!(
        requested = plan.count(),
        sent = counts.sent,
        batches = counts.batches,
        routing = plan.routing().describe(),
        pacing = %plan.pacing().describe(),
        end = end_name(&end),
        elapsed_ms = millis(elapsed),
        "Send run ended"
    );

    SendSummary {
        sent: counts.sent,
        batches: counts.batches,
        elapsed,
        end,
    }
}

async fn send_all<P: Publisher>(
    publisher: &P,
    plan: &SendPlan,
    next_event: &mut (impl FnMut(u64) -> anyhow::Result<OutgoingEvent> + Send),
    cancel: &AtomicBool,
    progress: &mut (impl FnMut(&SendProgress) + Send),
    counts: &mut Counts,
    started: Instant,
) -> SendEnd {
    let interval = plan.pacing().get();
    let per_batch = if interval.is_some() { 1 } else { u64::MAX };
    // The event that did not fit the previous batch, with its index.
    let mut carried: Option<(u64, OutgoingEvent)> = None;
    let mut previous_start: Option<Instant> = None;

    while counts.sent < plan.count() {
        if cancel.load(Ordering::Relaxed) {
            return SendEnd::Cancelled;
        }
        let waited = match (interval, previous_start) {
            (Some(interval), Some(previous)) => match pace(previous + interval, cancel).await {
                Some(waited) => waited,
                None => return SendEnd::Cancelled,
            },
            (None, _) | (_, None) => Duration::ZERO,
        };

        let batch_number = counts.batches + 1;
        let batch_start = Instant::now();
        previous_start = Some(batch_start);
        let created = tokio::select! {
            biased;
            created = publisher.create_batch(plan.routing()) => created,
            () = flag_set(cancel) => return SendEnd::Cancelled,
        };
        let batch = match created {
            Ok(batch) => batch,
            Err(error) => {
                return SendEnd::Failed(format!(
                    "Failed to create batch {batch_number}: {error:#}; no event of batch \
                     {batch_number} was sent"
                ));
            }
        };
        if counts.batches == 0 {
            info!(
                elapsed_ms = millis(batch_start.duration_since(started)),
                "First batch created; the sender is authorized"
            );
        }

        let remaining = (plan.count() - counts.sent).min(per_batch);
        let filled = fill(
            &batch,
            remaining,
            counts.sent,
            &mut *next_event,
            &mut carried,
        );
        let (added, bytes) = match filled {
            Ok(filled) => filled,
            Err(end) => return end,
        };

        let send_start = Instant::now();
        if let Err(error) = publisher.send_batch(batch).await {
            return SendEnd::Failed(format!(
                "{error:#}; a batch of {added} event(s) was not confirmed; the service may or \
                 may not have stored it"
            ));
        }
        counts.sent += added;
        counts.batches += 1;
        trace!(
            batch = counts.batches,
            events = added,
            bytes,
            send_ms = millis(send_start.elapsed()),
            wait_ms = millis(waited),
            "Batch sent"
        );
        progress(&SendProgress {
            sent: counts.sent,
            batches: counts.batches,
            elapsed: started.elapsed(),
        });
    }
    SendEnd::Completed
}

/// Waits until `deadline` unless `cancel` is set first. Returns how long it
/// waited, or `None` when cancelled. A deadline already past returns at once.
async fn pace(deadline: Instant, cancel: &AtomicBool) -> Option<Duration> {
    let now = Instant::now();
    if now >= deadline {
        return Some(Duration::ZERO);
    }
    tokio::select! {
        biased;
        () = flag_set(cancel) => None,
        () = tokio::time::sleep_until(deadline) => Some(deadline - now),
    }
}

/// Adds up to `limit` events to `batch`, starting with the event the previous
/// batch could not take. Returns how many were added and their body bytes,
/// leaving in `carried` the event that did not fit. Fails when an event
/// cannot be built or added, or when the first one does not fit even an
/// empty batch.
fn fill<B: PublishBatch>(
    batch: &B,
    limit: u64,
    first_index: u64,
    next_event: &mut impl FnMut(u64) -> anyhow::Result<OutgoingEvent>,
    carried: &mut Option<(u64, OutgoingEvent)>,
) -> Result<(u64, usize), SendEnd> {
    let mut added = 0;
    let mut bytes = 0;
    while added < limit {
        let (index, event) = match carried.take() {
            Some(carried) => carried,
            None => {
                let index = first_index + added;
                let event = next_event(index)
                    .with_context(|| format!("event {index}"))
                    .map_err(|error| SendEnd::Failed(format!("{error:#}")))?;
                (index, event)
            }
        };
        match batch.try_add(event.to_event_data()) {
            Ok(true) => {
                added += 1;
                bytes += event.body.len();
            }
            Ok(false) if added == 0 => {
                return Err(SendEnd::Failed(format!(
                    "event {index} ({}) does not fit in one batch; {SINGLE_EVENT_LIMIT}",
                    event.size_words()
                )));
            }
            Ok(false) => {
                *carried = Some((index, event));
                break;
            }
            Err(error) => {
                return Err(SendEnd::Failed(format!(
                    "{:#}",
                    error.context(format!("Failed to add event {index} to a batch"))
                )));
            }
        }
    }
    Ok((added, bytes))
}

fn end_name(end: &SendEnd) -> &'static str {
    match end {
        SendEnd::Completed => "completed",
        SendEnd::Cancelled => "cancelled",
        SendEnd::Failed(_) => "failed",
    }
}

/// Whole milliseconds, saturating (a run would need half a billion years to
/// overflow).
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
