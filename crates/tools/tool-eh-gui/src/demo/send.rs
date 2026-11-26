//! A demo send: no event hub, but the request's plan is followed, so pacing and Cancel behave
//! as they do against a real one.

use super::hub::{DEMO_HUB, DEMO_NAMESPACE};
use crate::archive::bulk::progress_due;
use crate::send::run::Names;
use crate::send::template::take_sequence;
use crate::send::{SendInput, SendRequest};
use chrono::Utc;
use eh_core::cancel::flag_set;
use eh_core::producer::{SendEnd, SendSummary};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::time::Instant;

/// How often a batched demo run moves on by a tenth of its count.
pub const BATCH_STEP_EVERY: Duration = Duration::from_millis(100);

/// Counts `request`'s events up to its plan's count without sending anything: a paced plan
/// one event per interval, the first at once; a batched one by a tenth of the count every
/// [`BATCH_STEP_EVERY`]. Every event counted takes its `{{sequence}}` number as a sent one
/// does. Ends `Cancelled` when the cancel flag is set before a step.
pub async fn demo_send(request: SendRequest, post: impl Fn(SendInput) + Send) {
    let SendRequest {
        run,
        plan,
        cancel,
        template,
        sequence,
        ..
    } = request;
    let started = Instant::now();
    post(SendInput::Started {
        run,
        target: Names {
            namespace: DEMO_NAMESPACE.to_owned(),
            event_hub: DEMO_HUB.to_owned(),
        },
        at: Utc::now(),
    });
    let count = plan.count();
    let (step, every) = match plan.pacing().get() {
        Some(interval) => (1, interval),
        None => (count.div_ceil(10), BATCH_STEP_EVERY),
    };
    let mut sent = 0;
    let mut batches = 0;
    let mut last_post = None;
    let mut end = SendEnd::Completed;
    while sent < count {
        if batches > 0 {
            tokio::select! {
                biased;
                () = flag_set(&cancel) => {}
                () = tokio::time::sleep(every) => {}
            }
        }
        if cancel.load(Ordering::Acquire) {
            end = SendEnd::Cancelled;
            break;
        }
        let before = sent;
        sent = (sent + step).min(count);
        for _ in before..sent {
            take_sequence(template.as_deref(), &sequence);
        }
        batches += 1;
        let now = Instant::now().into_std();
        if progress_due(last_post, now) {
            last_post = Some(now);
            post(SendInput::Progress { run, sent });
        }
    }
    post(SendInput::Finished {
        run,
        summary: SendSummary {
            sent,
            batches,
            elapsed: started.elapsed(),
            end,
        },
        close_error: None,
    });
}
