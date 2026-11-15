//! One send's run: preparing, sending, and how it ended, with the words the action row shows.

use super::compose::{events, pacing_words};
use crate::archive::bulk::CancelReason;
use crate::read::rows::display_line;
use chrono::{DateTime, Utc};
use common_utils::string_utils::format_thousands;
use shared_eventhub_core::producer::{Pacing, SendEnd, SendSummary};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;

/// A namespace and event hub, validated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Names {
    pub namespace: String,
    pub event_hub: String,
}

impl Names {
    /// `namespace/hub`, safe to show on one line.
    pub fn display(&self) -> String {
        display_line(format!("{}/{}", self.namespace, self.event_hub).chars())
    }

    /// The hub's name, safe to show on one line.
    pub fn hub(&self) -> String {
        display_line(self.event_hub.chars())
    }
}

/// Where the send run is. No state is final: the next Send leaves `Done`.
#[derive(Debug, Default)]
pub enum SendRun {
    #[default]
    Idle,
    /// Run `run` resolves its target and opens a producer; `target` is the destination named
    /// at the press, `None` when only the run can resolve it.
    Preparing {
        run: u64,
        cancel: Arc<AtomicBool>,
        cancel_requested: bool,
        requested: u64,
        pacing: Pacing,
        target: Option<Names>,
    },
    /// Run `run` sends to `target`, the names the producer resolved. `sent` is the last count
    /// the worker posted and `shown` the one on screen; `elapsed_secs` is whole seconds since
    /// `started`, as of the last tick.
    Sending {
        run: u64,
        cancel: Arc<AtomicBool>,
        cancel_requested: bool,
        requested: u64,
        pacing: Pacing,
        target: Names,
        sent: u64,
        shown: u64,
        started: DateTime<Utc>,
        elapsed_secs: i64,
    },
    /// Run `run` ended with `summary`; `close_error` is set when closing the producer failed.
    Done {
        run: u64,
        requested: u64,
        pacing: Pacing,
        target: Option<Names>,
        summary: SendSummary,
        close_error: Option<String>,
    },
}

/// Whether a run's end is a failure the user must see: the send failed, or closing the
/// connection did.
pub fn ended_failed(summary: &SendSummary, close_error: Option<&str>) -> bool {
    match summary.end {
        SendEnd::Failed(_) => true,
        SendEnd::Completed | SendEnd::Cancelled => close_error.is_some(),
    }
}

impl SendRun {
    /// Whether a run is preparing or sending.
    pub fn is_running(&self) -> bool {
        match self {
            Self::Preparing { .. } | Self::Sending { .. } => true,
            Self::Idle | Self::Done { .. } => false,
        }
    }

    /// The running run's id, when one runs.
    pub fn running_id(&self) -> Option<u64> {
        match self {
            Self::Preparing { run, .. } | Self::Sending { run, .. } => Some(*run),
            Self::Idle | Self::Done { .. } => None,
        }
    }

    /// Whether the running run was asked to stop.
    pub fn cancel_requested(&self) -> bool {
        match self {
            Self::Preparing {
                cancel_requested, ..
            }
            | Self::Sending {
                cancel_requested, ..
            } => *cancel_requested,
            Self::Idle | Self::Done { .. } => false,
        }
    }

    /// Asks the running run to stop; true when this changed anything. The only writer of the
    /// cancel flag and of `cancel_requested`.
    pub fn request_cancel(&mut self, reason: CancelReason) -> bool {
        match self {
            Self::Preparing {
                run,
                cancel,
                cancel_requested: requested @ false,
                ..
            }
            | Self::Sending {
                run,
                cancel,
                cancel_requested: requested @ false,
                ..
            } => {
                cancel.store(true, Ordering::Release);
                *requested = true;
                info!(
                    run = *run,
                    reason = reason.as_str(),
                    "send cancel requested"
                );
                true
            }
            Self::Preparing { .. } | Self::Sending { .. } | Self::Idle | Self::Done { .. } => false,
        }
    }

    /// Moves the shown count and the elapsed time to `now`; true when either changed.
    pub fn tick(&mut self, now: DateTime<Utc>) -> bool {
        match self {
            Self::Sending {
                sent,
                shown,
                started,
                elapsed_secs,
                ..
            } => {
                let elapsed = (now - *started).num_seconds().max(*elapsed_secs);
                let changed = *shown != *sent || elapsed != *elapsed_secs;
                *shown = *sent;
                *elapsed_secs = elapsed;
                changed
            }
            Self::Idle | Self::Preparing { .. } | Self::Done { .. } => false,
        }
    }

    /// The `Done` line, or `None` while no run has ended.
    pub fn outcome_line(&self) -> Option<String> {
        let (requested, target, summary) = match self {
            Self::Done {
                requested,
                target,
                summary,
                ..
            } => (requested, target, summary),
            Self::Idle | Self::Preparing { .. } | Self::Sending { .. } => return None,
        };
        let requested = format_thousands(*requested);
        let sent = format_thousands(summary.sent);
        Some(match &summary.end {
            SendEnd::Completed => format!("Sent {}", events(summary.sent)),
            SendEnd::Cancelled if summary.sent == 0 => "Canceled before sending".to_owned(),
            SendEnd::Cancelled => format!("Canceled after {sent} of {requested}"),
            SendEnd::Failed(reason) if summary.sent == 0 => {
                let reason = display_line(reason.chars());
                match target {
                    Some(target) => format!("Couldn't send to {}: {reason}", target.hub()),
                    None => format!("Couldn't send: {reason}"),
                }
            }
            SendEnd::Failed(reason) => format!(
                "Stopped after {sent} of {requested}: {}",
                display_line(reason.chars())
            ),
        })
    }

    /// The running line's words around the rolling count: what comes before it and after it.
    pub fn running_words(&self) -> Option<(String, String)> {
        match self {
            Self::Preparing { target, .. } => Some((
                match target {
                    Some(target) => format!("Connecting to {}", target.hub()),
                    None => "Connecting".to_owned(),
                },
                String::new(),
            )),
            Self::Sending {
                requested,
                pacing,
                target,
                ..
            } => {
                let pacing = pacing.get().map(pacing_words).unwrap_or_default();
                Some((
                    "Sending".to_owned(),
                    format!(
                        "of {} to {}{pacing}",
                        format_thousands(*requested),
                        target.hub()
                    ),
                ))
            }
            Self::Idle | Self::Done { .. } => None,
        }
    }
}
