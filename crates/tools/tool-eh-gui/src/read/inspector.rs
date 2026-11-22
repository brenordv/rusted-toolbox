//! The inspector's model: the selected message, what the body area shows, copy feedback, and
//! exporting one message. Each section owns one [`InspectorState`].

use super::Selected;
use super::rows::{cut_at_char_boundary, key_string};
use crate::design::tokens::{INSPECTOR_MAX, INSPECTOR_MIN, INSPECTOR_WIDTH};
use crate::state::{ListSection, ToastKind};
use chrono::{DateTime, TimeDelta, Utc};
use common_utils::string_utils::format_bytes_to_string;
use rune_kit::PaneState;
use shared_eventhub_core::models::{CapturedMessage, MessageKey};
use shared_eventhub_persistence::export::{ExportFormat, ExportOptions};
use shared_eventhub_persistence::paths::DataPaths;
use std::path::{Path, PathBuf};

/// The most body bytes the inspector shows; copying takes the whole body.
pub const BODY_DISPLAY_CAP: usize = 256 * 1024;
/// How long a "Copied" or "Exported" label stays.
pub const FEEDBACK_FOR: TimeDelta = TimeDelta::milliseconds(1_500);

/// Where exporting the selected message is. `owner` is the token of the rows the message came
/// from (a session in the Read section, a data folder in the Export section); a dialog answer
/// or a result for another owner is not this export's.
#[derive(Debug, Default)]
pub enum ExportPhase {
    #[default]
    Idle,
    /// The save dialog is open for `key`.
    Picking {
        owner: u64,
        key: MessageKey,
        key_str: String,
        paths: DataPaths,
    },
    /// The file is being written.
    Running {
        owner: u64,
        key: MessageKey,
        key_str: String,
    },
}

impl ExportPhase {
    pub fn is_idle(&self) -> bool {
        match self {
            Self::Idle => true,
            Self::Picking { .. } | Self::Running { .. } => false,
        }
    }

    pub fn is_running(&self) -> bool {
        match self {
            Self::Running { .. } => true,
            Self::Idle | Self::Picking { .. } => false,
        }
    }
}

/// What a feedback label says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackKind {
    CopiedBody,
    CopiedJson,
    Exported,
}

/// A short-lived label on one message's action button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feedback {
    pub key: MessageKey,
    pub kind: FeedbackKind,
    pub until: DateTime<Utc>,
}

impl Feedback {
    pub fn new(key: MessageKey, kind: FeedbackKind, now: DateTime<Utc>) -> Self {
        Self {
            key,
            kind,
            until: now + FEEDBACK_FOR,
        }
    }

    /// Whether the label shows `kind` for `key`.
    pub fn shows(&self, key: &MessageKey, kind: FeedbackKind) -> bool {
        self.key == *key && self.kind == kind
    }
}

/// Something that happened in an inspector.
#[derive(Debug)]
pub enum InspectorInput {
    /// The selected message's body or JSON reached the clipboard.
    Copied {
        kind: FeedbackKind,
        now: DateTime<Utc>,
    },
    CopyFailed(String),
    ExportPressed,
    /// The save dialog opened for `key` of `owner` answered.
    ExportAnswered {
        owner: u64,
        key: MessageKey,
        answer: Result<Option<PathBuf>, String>,
    },
    ExportDone {
        owner: u64,
        key: MessageKey,
        result: Result<(), String>,
        now: DateTime<Utc>,
    },
}

/// Write one archived message to a file the user chose; the result goes back to `section`.
#[derive(Debug)]
pub struct ExportOneRequest {
    pub section: ListSection,
    pub owner: u64,
    pub key: MessageKey,
    pub key_str: String,
    pub options: ExportOptions,
    pub destination: PathBuf,
    pub paths: DataPaths,
}

/// What an inspector transition did.
#[derive(Debug, Default)]
pub struct InspectorOutcome {
    pub export: Option<ExportOneRequest>,
    pub changed: bool,
    pub toasts: Vec<(ToastKind, String)>,
}

impl InspectorOutcome {
    pub(crate) fn changed() -> Self {
        Self {
            changed: true,
            ..Self::default()
        }
    }
}

/// One section's inspector: the selected message, its copy feedback, exporting it, the export
/// picks, and the pane.
#[derive(Debug)]
pub struct InspectorState {
    pub selected: Option<Selected>,
    pub feedback: Option<Feedback>,
    pub export: ExportPhase,
    pub format: ExportFormat,
    pub metadata: bool,
    pub pane: PaneState,
}

impl Default for InspectorState {
    fn default() -> Self {
        Self::with_export(ExportFormat::Txt, true)
    }
}

impl InspectorState {
    /// A closed inspector whose exports start as `format`, with or without metadata.
    pub fn with_export(format: ExportFormat, metadata: bool) -> Self {
        Self {
            selected: None,
            feedback: None,
            export: ExportPhase::Idle,
            format,
            metadata,
            pane: PaneState::new(INSPECTOR_WIDTH)
                .range(INSPECTOR_MIN, INSPECTOR_MAX)
                .collapsed(),
        }
    }

    /// The rows it showed are gone: no selection or feedback, the pane closed, an open save
    /// dialog dropped. An export still writing a file stays, and so do the picks and the pane's
    /// width.
    pub fn reset(&mut self) {
        self.selected = None;
        self.feedback = None;
        self.export = running_only(std::mem::take(&mut self.export));
        self.pane.set_open(false);
    }

    pub fn select(&mut self, selected: Selected) {
        self.selected = Some(selected);
        self.pane.set_open(true);
    }

    pub fn clear_selection(&mut self) {
        self.selected = None;
        self.pane.set_open(false);
    }

    /// Ends feedback whose time is up; true when something shown changed.
    pub fn tick(&mut self, now: DateTime<Utc>) -> bool {
        if self.feedback.as_ref().is_some_and(|f| now >= f.until) {
            self.feedback = None;
            return true;
        }
        false
    }
}

/// `phase` if it is still writing a file, `Idle` otherwise.
pub(crate) fn running_only(phase: ExportPhase) -> ExportPhase {
    match phase {
        running @ ExportPhase::Running { .. } => running,
        ExportPhase::Idle | ExportPhase::Picking { .. } => ExportPhase::Idle,
    }
}

/// The part of `body` the inspector shows, and the caption saying it was cut.
pub fn shown_body(body: &str) -> (&str, Option<String>) {
    if body.len() <= BODY_DISPLAY_CAP {
        return (body, None);
    }
    let caption = format!(
        "Showing the first {}. Copy body copies all {}.",
        format_bytes_to_string(BODY_DISPLAY_CAP as u64),
        format_bytes_to_string(body.len() as u64)
    );
    (cut_at_char_boundary(body, BODY_DISPLAY_CAP), Some(caption))
}

/// `message` as pretty JSON, with its own field names.
///
/// # Errors
/// Fails when serialization fails; the text is shown in a toast.
pub fn message_json(message: &CapturedMessage) -> Result<String, String> {
    serde_json::to_string_pretty(message)
        .map_err(|error| format!("Could not write the message as JSON: {error}"))
}

/// The suggested file name for exporting `key` as `format`.
pub fn export_file_name(key: &MessageKey, format: ExportFormat) -> String {
    format!("{}.{}", key_string(key), format.extension())
}

/// The options for writing one message to `destination`. The archive's export log is not
/// touched by a single-message export, so the skip and condense settings stay off.
pub fn export_options(format: ExportFormat, metadata: bool, destination: &Path) -> ExportOptions {
    ExportOptions {
        format,
        include_metadata: metadata,
        condense_output: false,
        use_local_time: false,
        export_folder: destination
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
        skip_already_exported: false,
    }
}

#[cfg(test)]
mod tests;
