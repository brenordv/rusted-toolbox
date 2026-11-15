//! The inspector's model: what the body area shows, copy feedback, and exporting one message.

use super::rows::key_string;
use chrono::{DateTime, TimeDelta, Utc};
use common_utils::string_utils::format_bytes_to_string;
use shared_eventhub_core::models::{CapturedMessage, MessageKey};
use shared_eventhub_persistence::export::{ExportFormat, ExportOptions};
use shared_eventhub_persistence::paths::DataPaths;
use std::path::Path;

/// The most body bytes the inspector shows; copying takes the whole body.
pub const BODY_DISPLAY_CAP: usize = 256 * 1024;
/// How long a "Copied" or "Exported" label stays.
pub const FEEDBACK_FOR: TimeDelta = TimeDelta::milliseconds(1_500);

/// Where exporting the selected message is.
#[derive(Debug, Default)]
pub enum ExportPhase {
    #[default]
    Idle,
    /// The save dialog is open for `key` of `session`.
    Picking {
        session: u64,
        key: MessageKey,
        key_str: String,
        paths: DataPaths,
    },
    /// The file is being written.
    Running {
        session: u64,
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

/// The part of `body` the inspector shows, and the caption saying it was cut.
pub fn shown_body(body: &str) -> (&str, Option<String>) {
    if body.len() <= BODY_DISPLAY_CAP {
        return (body, None);
    }
    let mut end = BODY_DISPLAY_CAP;
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    let caption = format!(
        "Showing the first {}. Copy body copies all {}.",
        format_bytes_to_string(BODY_DISPLAY_CAP as u64),
        format_bytes_to_string(body.len() as u64)
    );
    (&body[..end], Some(caption))
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
