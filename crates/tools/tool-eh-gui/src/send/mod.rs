//! The Send section's model: the composer, the destination it names, and the run of one send.
//! Plain Rust, no window: inputs change the state and come back with the effects to run.

pub mod compose;
pub mod run;
pub mod template;

use crate::archive::bulk::CancelReason;
use crate::form::ConnectForm;
use crate::services::ConnectRequest;
use crate::state::ToastKind;
use chrono::{DateTime, Utc};
use common_mock_data::template::Template;
use compose::{Composed, Composer, SendField, SendIssues};
use eh_core::connection::target_names;
use eh_core::producer::{OutgoingEvent, SendEnd, SendPlan, SendSummary};
use eh_persistence::config::{AuthMethodConfig, resolve_target_with};
use eh_persistence::paths::DataPaths;
use run::{Names, SendRun, ended_failed};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tracing::{debug, info, warn};

/// The destination a connection form resolves to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Destination {
    /// The form has no usable target yet.
    #[default]
    Missing,
    /// The names the form resolves to.
    Named(Names),
    /// A typed connection string core refuses; its error is never shown.
    Invalid,
    /// A saved connection string or a Key Vault secret: the names are known once a run
    /// resolves them.
    Stored,
}

impl Destination {
    /// The destination `form` resolves to, worked out without the secrets file or the network.
    pub fn of(form: &ConnectForm) -> Self {
        if !form.issues().is_empty() {
            return Self::Missing;
        }
        let (config, given) = form.to_config();
        match &config.target.auth {
            AuthMethodConfig::ConnectionString { .. } if given.connection_string.is_none() => {
                return Self::Stored;
            }
            AuthMethodConfig::ConnectionString { .. }
            | AuthMethodConfig::Credential { .. }
            | AuthMethodConfig::KeyVaultConnectionString { .. } => {}
        }
        let Ok(target) = resolve_target_with(&config, given, None) else {
            return Self::Invalid;
        };
        match target_names(&target) {
            None => Self::Stored,
            Some(Ok((namespace, event_hub))) => Self::Named(Names {
                namespace,
                event_hub,
            }),
            Some(Err(_)) => Self::Invalid,
        }
    }

    /// Whether a send can start from this destination.
    pub fn is_ready(&self) -> bool {
        match self {
            Self::Named(_) | Self::Stored => true,
            Self::Missing | Self::Invalid => false,
        }
    }

    /// The names, when the form gives them.
    pub fn names(&self) -> Option<&Names> {
        match self {
            Self::Named(names) => Some(names),
            Self::Missing | Self::Invalid | Self::Stored => None,
        }
    }
}

/// Something that happened in the Send section.
#[derive(Debug)]
pub enum SendInput {
    SendPressed,
    CancelPressed,
    /// Run `run` opened its producer for `target` at `at`.
    Started {
        run: u64,
        target: Names,
        at: DateTime<Utc>,
    },
    Progress {
        run: u64,
        sent: u64,
    },
    Finished {
        run: u64,
        summary: SendSummary,
        close_error: Option<String>,
    },
}

/// Work the Send section asks for.
#[derive(Debug)]
pub enum SendEffect {
    StartSend(SendRequest),
}

/// Send `event` as `plan` says to the target `target` describes; results go back tagged `run`.
/// With `template`, each event is a fresh render of it under `event`'s content type, and an
/// event that shows `{{sequence}}` takes its number from `sequence`.
#[derive(Debug)]
pub struct SendRequest {
    pub run: u64,
    pub target: ConnectRequest,
    pub event: OutgoingEvent,
    pub template: Option<Arc<Template>>,
    pub plan: SendPlan,
    pub cancel: Arc<AtomicBool>,
    pub sequence: Arc<AtomicU64>,
}

/// What a Send transition did.
#[derive(Debug, Default)]
pub struct SendOutcome {
    pub effects: Vec<SendEffect>,
    pub changed: bool,
    /// The run ended, a failure when `failed`, with this line.
    pub ended: Option<(bool, String)>,
}

impl SendOutcome {
    fn changed() -> Self {
        Self {
            changed: true,
            ..Self::default()
        }
    }
}

/// What a Send press may depend on outside the section: the form and the data folder it
/// resolves to.
pub struct PressContext<'a> {
    pub sheet_open: bool,
    pub window_closed: bool,
    pub form: &'a ConnectForm,
    pub paths: DataPaths,
}

/// The Send section's state.
#[derive(Debug, Default)]
pub struct SendState {
    pub composer: Composer,
    /// Set by the first Send press with issues, so an untouched composer shows none.
    pub show_issues: bool,
    /// What the connection form names, recomputed only when the form may have changed.
    pub destination: Destination,
    pub run: SendRun,
    /// The part to focus after a press with issues, until taken.
    pending_focus: Option<SendField>,
    next_run: u64,
}

impl SendState {
    /// Recomputes the destination from `form`.
    pub fn refresh_destination(&mut self, form: &ConnectForm) {
        self.destination = Destination::of(form);
    }

    /// The composer's issues as they show now: none until the first press with issues.
    pub fn shown_issues(&self) -> SendIssues {
        if self.show_issues {
            self.composer.issues(self.destination.is_ready())
        } else {
            SendIssues::default()
        }
    }

    /// The part focus should move to, once.
    pub fn take_focus(&mut self) -> Option<SendField> {
        self.pending_focus.take()
    }

    /// Whether a run is preparing or sending.
    pub fn is_running(&self) -> bool {
        self.run.is_running()
    }

    /// Asks the running run to stop; true when this changed anything.
    pub fn request_cancel(&mut self, reason: CancelReason) -> bool {
        self.run.request_cancel(reason)
    }

    /// Moves the shown count and elapsed time; true when something shown changed.
    pub fn tick(&mut self, now: DateTime<Utc>) -> bool {
        self.run.tick(now)
    }

    fn press(&mut self, cx: PressContext<'_>) -> SendOutcome {
        let closed_placeholders = std::mem::replace(&mut self.composer.placeholders_open, false);
        if cx.sheet_open || cx.window_closed || self.run.is_running() {
            debug!(
                sheet_open = cx.sheet_open,
                window_closed = cx.window_closed,
                running = self.run.is_running(),
                "Send ignored"
            );
            return SendOutcome {
                changed: closed_placeholders,
                ..SendOutcome::default()
            };
        }
        let Composed {
            plan,
            event,
            template,
        } = match self.composer.compose(self.destination.is_ready()) {
            Ok(composed) => composed,
            Err(issues) => {
                debug!(
                    issues = issues.len(),
                    "Send refused: the composer has issues"
                );
                self.show_issues = true;
                self.pending_focus = issues.first();
                return SendOutcome::changed();
            }
        };
        self.next_run += 1;
        let run = self.next_run;
        let cancel = Arc::new(AtomicBool::new(false));
        let sequence = Arc::clone(self.composer.sequence());
        let sequence_from = sequence_from(template.as_deref(), &sequence);
        info!(
            run,
            requested = plan.count(),
            partition_key = plan.routing().key().is_some(),
            pacing = %plan.pacing().describe(),
            template = template.is_some(),
            sequence_from,
            "send started"
        );
        let (config, given) = cx.form.to_config();
        self.run = SendRun::Preparing {
            run,
            cancel: Arc::clone(&cancel),
            cancel_requested: false,
            requested: plan.count(),
            pacing: plan.pacing(),
            target: self.destination.names().cloned(),
        };
        SendOutcome {
            effects: vec![SendEffect::StartSend(SendRequest {
                run,
                target: ConnectRequest {
                    config,
                    given,
                    paths: cx.paths,
                },
                event,
                template,
                plan,
                cancel,
                sequence,
            })],
            ..SendOutcome::changed()
        }
    }

    /// Applies `input`; `cx` is what a press depends on outside the section.
    pub fn apply(&mut self, input: SendInput, cx: PressContext<'_>) -> SendOutcome {
        match input {
            SendInput::SendPressed => self.press(cx),
            SendInput::CancelPressed => SendOutcome {
                changed: self.request_cancel(CancelReason::Button),
                ..SendOutcome::default()
            },
            SendInput::Started { run, target, at } => self.started(run, target, at),
            SendInput::Progress { run, sent } => self.progress(run, sent),
            SendInput::Finished {
                run,
                summary,
                close_error,
            } => self.finished(run, summary, close_error),
        }
    }

    fn started(&mut self, run: u64, target: Names, at: DateTime<Utc>) -> SendOutcome {
        match std::mem::take(&mut self.run) {
            SendRun::Preparing {
                run: current,
                cancel,
                cancel_requested,
                requested,
                pacing,
                ..
            } if current == run => {
                debug!(run, "send connected");
                self.run = SendRun::Sending {
                    run,
                    cancel,
                    cancel_requested,
                    requested,
                    pacing,
                    target,
                    sent: 0,
                    shown: 0,
                    started: at,
                    elapsed_secs: 0,
                };
                SendOutcome::changed()
            }
            other @ (SendRun::Idle
            | SendRun::Preparing { .. }
            | SendRun::Sending { .. }
            | SendRun::Done { .. }) => {
                debug!(run, "a send start nothing waits for was dropped");
                self.run = other;
                SendOutcome::default()
            }
        }
    }

    fn progress(&mut self, run: u64, latest: u64) -> SendOutcome {
        match &mut self.run {
            SendRun::Sending {
                run: current, sent, ..
            } if *current == run => {
                *sent = latest;
            }
            SendRun::Sending { .. }
            | SendRun::Idle
            | SendRun::Preparing { .. }
            | SendRun::Done { .. } => {
                debug!(run, "a send progress update nothing waits for was dropped");
            }
        }
        SendOutcome::default()
    }

    fn finished(
        &mut self,
        run: u64,
        summary: SendSummary,
        close_error: Option<String>,
    ) -> SendOutcome {
        let (requested, pacing, target) = match &self.run {
            SendRun::Preparing {
                run: current,
                requested,
                pacing,
                target,
                ..
            } if *current == run => (*requested, *pacing, target.clone()),
            SendRun::Sending {
                run: current,
                requested,
                pacing,
                target,
                ..
            } if *current == run => (*requested, *pacing, Some(target.clone())),
            SendRun::Idle
            | SendRun::Preparing { .. }
            | SendRun::Sending { .. }
            | SendRun::Done { .. } => {
                debug!(run, "the end of a send nothing waits for was dropped");
                return SendOutcome::default();
            }
        };
        self.composer.resample_sequence();
        let failed = ended_failed(&summary, close_error.as_deref());
        info!(
            run,
            requested,
            sent = summary.sent,
            batches = summary.batches,
            end = end_name(&summary),
            elapsed_ms = summary.elapsed.as_millis(),
            "send ended"
        );
        if failed {
            let error = match &summary.end {
                SendEnd::Failed(reason) => reason.as_str(),
                SendEnd::Completed | SendEnd::Cancelled => {
                    close_error.as_deref().unwrap_or_default()
                }
            };
            warn!(run, sent = summary.sent, requested, error = %error, "the send failed");
        }
        self.run = SendRun::Done {
            run,
            requested,
            pacing,
            target,
            summary,
            close_error,
        };
        let line = self.run.outcome_line().unwrap_or_default();
        SendOutcome {
            ended: Some((failed, line)),
            ..SendOutcome::changed()
        }
    }
}

/// The first number a run takes from `sequence`, when `template` uses `{{sequence}}`.
fn sequence_from(template: Option<&Template>, sequence: &AtomicU64) -> Option<u64> {
    template
        .filter(|template| template.uses_sequence())
        .map(|_| sequence.load(Ordering::Relaxed))
}

fn end_name(summary: &SendSummary) -> &'static str {
    match summary.end {
        SendEnd::Completed => "completed",
        SendEnd::Cancelled => "cancelled",
        SendEnd::Failed(_) => "failed",
    }
}

/// The toast for a run that ended where its action row cannot be seen.
pub fn ended_toast(failed: bool, line: String) -> (ToastKind, String) {
    if failed {
        (ToastKind::Danger, line)
    } else {
        (ToastKind::Info, line)
    }
}

#[cfg(test)]
mod tests;
