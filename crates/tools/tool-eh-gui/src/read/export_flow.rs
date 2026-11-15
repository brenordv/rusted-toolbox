//! Exporting the selected message: the save dialog, the background write, and its result.

use super::inspector::{ExportPhase, Feedback, FeedbackKind, export_options};
use super::rows::key_string;
use super::{ExportOneRequest, ReadEffect, ReadOutcome, ReadState};
use crate::bridge::EngineSession;
use crate::state::{ConnectionState, ToastKind};
use chrono::{DateTime, Utc};
use shared_eventhub_core::models::MessageKey;
use shared_eventhub_persistence::paths::DataPaths;
use std::path::PathBuf;
use tracing::{debug, info, warn};

impl ReadState {
    /// Export pressed: the save dialog opens for the selection, unless an export is under way.
    pub(super) fn export_pressed(
        &mut self,
        connection: &ConnectionState,
        paths: DataPaths,
    ) -> ReadOutcome {
        let (Some(selected), Some(session), true) = (
            &self.selected,
            connection.session().map(EngineSession::id),
            self.export.is_idle(),
        ) else {
            debug!("export ignored: no selection, no session, or an export already runs");
            return ReadOutcome::default();
        };
        self.export = ExportPhase::Picking {
            session,
            key: selected.key.clone(),
            key_str: selected.key_str.clone(),
            paths,
        };
        ReadOutcome::changed()
    }

    /// The save dialog opened for `key` of `session` answered; any other answer is dropped.
    pub(super) fn export_answered(
        &mut self,
        session: u64,
        key: &MessageKey,
        answer: Result<Option<PathBuf>, String>,
    ) -> ReadOutcome {
        let (picking_session, picking_key, key_str, paths) = match std::mem::take(&mut self.export)
        {
            ExportPhase::Picking {
                session,
                key,
                key_str,
                paths,
            } => (session, key, key_str, paths),
            phase @ (ExportPhase::Idle | ExportPhase::Running { .. }) => {
                self.export = phase;
                return late_answer(session, &answer);
            }
        };
        if picking_session != session || picking_key != *key {
            debug!(
                session,
                "a save dialog answer for another export was dropped"
            );
            self.export = ExportPhase::Picking {
                session: picking_session,
                key: picking_key,
                key_str,
                paths,
            };
            return ReadOutcome::default();
        }
        match answer {
            Ok(None) => ReadOutcome::changed(),
            Err(error) => dialog_failed(&error),
            Ok(Some(destination)) => {
                let options =
                    export_options(self.export_format, self.export_metadata, &destination);
                self.export = ExportPhase::Running {
                    session,
                    key: picking_key.clone(),
                    key_str: key_str.clone(),
                };
                ReadOutcome {
                    effects: vec![ReadEffect::ExportOne(ExportOneRequest {
                        session,
                        key: picking_key,
                        key_str,
                        options,
                        destination,
                        paths,
                    })],
                    ..ReadOutcome::changed()
                }
            }
        }
    }

    /// A background export ended. A failure always shows; "Exported" shows only on the message
    /// it was for, in its session.
    pub(super) fn export_done(
        &mut self,
        session: u64,
        key: MessageKey,
        result: Result<(), String>,
        now: DateTime<Utc>,
    ) -> ReadOutcome {
        let key_str = key_string(&key);
        let ours = match &self.export {
            ExportPhase::Running {
                session: running_session,
                key: running_key,
                ..
            } => *running_session == session && *running_key == key,
            ExportPhase::Idle | ExportPhase::Picking { .. } => false,
        };
        if ours {
            self.export = ExportPhase::Idle;
        }
        let format = self.export_format;
        match result {
            Ok(()) => {
                info!(
                    session,
                    key = %key_str,
                    format = ?format,
                    metadata = self.export_metadata,
                    "exported one message"
                );
                let selected = self.selected.as_ref().is_some_and(|s| s.key == key);
                if selected && self.session == Some(session) {
                    self.feedback = Some(Feedback::new(key, FeedbackKind::Exported, now));
                }
                ReadOutcome::changed()
            }
            Err(error) => {
                warn!(
                    session,
                    key = %key_str,
                    format = ?format,
                    error = %error,
                    "exporting one message failed"
                );
                ReadOutcome {
                    toasts: vec![(
                        ToastKind::Danger,
                        format!("Could not export the message: {error}"),
                    )],
                    ..ReadOutcome::changed()
                }
            }
        }
    }
}

/// A save dialog answered after its export was dropped (its session ended). A picked file is
/// not written, and the user is told; a cancel needs nothing.
fn late_answer(session: u64, answer: &Result<Option<PathBuf>, String>) -> ReadOutcome {
    debug!(
        session,
        "a save dialog answer with no dialog open was dropped"
    );
    match answer {
        Ok(Some(_)) => ReadOutcome {
            toasts: vec![(
                ToastKind::Info,
                "The session ended before the export started; nothing was written".to_owned(),
            )],
            ..ReadOutcome::changed()
        },
        Err(error) => dialog_failed(error),
        Ok(None) => ReadOutcome::default(),
    }
}

/// The save dialog could not be shown: a danger toast with the reason.
fn dialog_failed(error: &str) -> ReadOutcome {
    warn!(error = %error, "the save dialog failed");
    ReadOutcome {
        toasts: vec![(
            ToastKind::Danger,
            format!("Could not open the save dialog: {error}"),
        )],
        ..ReadOutcome::changed()
    }
}

/// `phase` if it is still writing a file, `Idle` otherwise.
pub(super) fn running_only(phase: ExportPhase) -> ExportPhase {
    match phase {
        running @ ExportPhase::Running { .. } => running,
        ExportPhase::Idle | ExportPhase::Picking { .. } => ExportPhase::Idle,
    }
}
