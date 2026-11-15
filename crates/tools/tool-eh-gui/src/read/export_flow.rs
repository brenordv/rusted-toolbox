//! An inspector's transitions: copy feedback, the save dialog, the background write, and its
//! result.

use super::inspector::{
    ExportOneRequest, ExportPhase, Feedback, FeedbackKind, InspectorInput, InspectorOutcome,
    InspectorState, export_options,
};
use super::rows::key_string;
use crate::state::{ListSection, ToastKind};
use chrono::{DateTime, Utc};
use shared_eventhub_core::models::MessageKey;
use shared_eventhub_persistence::paths::DataPaths;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// What the section an inspector belongs to tells each transition: which section it is, the
/// token of the rows' current source (a session or a data folder; `None` when there is none),
/// the data folder an export reads, and what to say when a file is picked after its export was
/// dropped.
#[derive(Clone, Copy, Debug)]
pub struct InspectorContext<'a> {
    pub section: ListSection,
    pub owner: Option<u64>,
    pub paths: &'a DataPaths,
    pub dropped_answer: &'static str,
}

impl InspectorState {
    /// Applies `input` for the section `cx` describes.
    pub fn apply(&mut self, input: InspectorInput, cx: InspectorContext<'_>) -> InspectorOutcome {
        match input {
            InspectorInput::Copied { kind, now } => {
                if let Some(selected) = &self.selected {
                    self.feedback = Some(Feedback::new(selected.key.clone(), kind, now));
                }
                InspectorOutcome::changed()
            }
            InspectorInput::CopyFailed(text) => InspectorOutcome {
                toasts: vec![(ToastKind::Danger, text)],
                ..InspectorOutcome::changed()
            },
            InspectorInput::ExportPressed => self.export_pressed(cx),
            InspectorInput::ExportAnswered { owner, key, answer } => {
                self.export_answered(owner, &key, answer, cx)
            }
            InspectorInput::ExportDone {
                owner,
                key,
                result,
                now,
            } => self.export_done(owner, key, result, now, cx),
        }
    }

    /// Export pressed: the save dialog opens for the selection, unless an export is under way.
    fn export_pressed(&mut self, cx: InspectorContext<'_>) -> InspectorOutcome {
        let (Some(selected), Some(owner), true) = (&self.selected, cx.owner, self.export.is_idle())
        else {
            debug!(
                section = ?cx.section,
                "export ignored: no selection, no rows, or an export already runs"
            );
            return InspectorOutcome::default();
        };
        self.export = ExportPhase::Picking {
            owner,
            key: selected.key.clone(),
            key_str: selected.key_str.clone(),
            paths: cx.paths.clone(),
        };
        InspectorOutcome::changed()
    }

    /// The save dialog opened for `key` of `owner` answered; any other answer is dropped.
    fn export_answered(
        &mut self,
        owner: u64,
        key: &MessageKey,
        answer: Result<Option<PathBuf>, String>,
        cx: InspectorContext<'_>,
    ) -> InspectorOutcome {
        let (picking_owner, picking_key, key_str, paths) = match std::mem::take(&mut self.export) {
            ExportPhase::Picking {
                owner,
                key,
                key_str,
                paths,
            } => (owner, key, key_str, paths),
            phase @ (ExportPhase::Idle | ExportPhase::Running { .. }) => {
                self.export = phase;
                return late_answer(owner, &answer, cx);
            }
        };
        if picking_owner != owner || picking_key != *key {
            debug!(
                section = ?cx.section,
                owner, "a save dialog answer for another export was dropped"
            );
            self.export = ExportPhase::Picking {
                owner: picking_owner,
                key: picking_key,
                key_str,
                paths,
            };
            return InspectorOutcome::default();
        }
        match answer {
            Ok(None) => InspectorOutcome::changed(),
            Err(error) => dialog_failed(&error),
            Ok(Some(destination)) => {
                let options = export_options(self.format, self.metadata, &destination);
                self.export = ExportPhase::Running {
                    owner,
                    key: picking_key.clone(),
                    key_str: key_str.clone(),
                };
                InspectorOutcome {
                    export: Some(ExportOneRequest {
                        section: cx.section,
                        owner,
                        key: picking_key,
                        key_str,
                        options,
                        destination,
                        paths,
                    }),
                    ..InspectorOutcome::changed()
                }
            }
        }
    }

    /// A background export ended. A failure always shows; "Exported" shows only on the message
    /// it was for, while its rows are still the ones shown.
    fn export_done(
        &mut self,
        owner: u64,
        key: MessageKey,
        result: Result<(), String>,
        now: DateTime<Utc>,
        cx: InspectorContext<'_>,
    ) -> InspectorOutcome {
        let key_str = key_string(&key);
        let ours = match &self.export {
            ExportPhase::Running {
                owner: running_owner,
                key: running_key,
                ..
            } => *running_owner == owner && *running_key == key,
            ExportPhase::Idle | ExportPhase::Picking { .. } => false,
        };
        if ours {
            self.export = ExportPhase::Idle;
        }
        let format = self.format;
        match result {
            Ok(()) => {
                info!(
                    section = ?cx.section,
                    owner,
                    key = %key_str,
                    format = ?format,
                    metadata = self.metadata,
                    "exported one message"
                );
                let selected = self.selected.as_ref().is_some_and(|s| s.key == key);
                if selected && cx.owner == Some(owner) {
                    self.feedback = Some(Feedback::new(key, FeedbackKind::Exported, now));
                }
                InspectorOutcome::changed()
            }
            Err(error) => {
                warn!(
                    section = ?cx.section,
                    owner,
                    key = %key_str,
                    format = ?format,
                    error = %error,
                    "exporting one message failed"
                );
                InspectorOutcome {
                    toasts: vec![(
                        ToastKind::Danger,
                        format!("Could not export the message: {error}"),
                    )],
                    ..InspectorOutcome::changed()
                }
            }
        }
    }
}

/// A save dialog answered after its export was dropped. A picked file is not written, and the
/// user is told; a cancel needs nothing.
fn late_answer(
    owner: u64,
    answer: &Result<Option<PathBuf>, String>,
    cx: InspectorContext<'_>,
) -> InspectorOutcome {
    debug!(
        section = ?cx.section,
        owner, "a save dialog answer with no dialog open was dropped"
    );
    match answer {
        Ok(Some(_)) => InspectorOutcome {
            toasts: vec![(ToastKind::Info, cx.dropped_answer.to_owned())],
            ..InspectorOutcome::changed()
        },
        Err(error) => dialog_failed(error),
        Ok(None) => InspectorOutcome::default(),
    }
}

/// The save dialog could not be shown: a danger toast with the reason.
fn dialog_failed(error: &str) -> InspectorOutcome {
    warn!(error = %error, "the save dialog failed");
    InspectorOutcome {
        toasts: vec![(
            ToastKind::Danger,
            format!("Could not open the save dialog: {error}"),
        )],
        ..InspectorOutcome::changed()
    }
}
