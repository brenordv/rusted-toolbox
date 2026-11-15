//! Failure text in place and in full: the preview every surface shows, and the error details
//! modal that shows and copies the whole text.

use super::{AppState, Outcome};
use crate::read::inspector::FEEDBACK_FOR;
use crate::read::rows::display_line;
use chrono::{DateTime, Utc};
use tracing::debug;

/// The most characters of a failure text a surface shows in place.
pub const ERROR_PREVIEW_CHARS: usize = 140;

/// A failure text as a surface shows it in place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorPreview {
    /// At most [`ERROR_PREVIEW_CHARS`] characters on one line, ending in "…" when cut.
    pub shown: String,
    /// Whether characters were left out, so the surface offers the whole text.
    pub cut: bool,
}

/// `text` on one line (control and bidirectional characters as spaces), cut after
/// [`ERROR_PREVIEW_CHARS`] characters with "…". The cut drops trailing spaces and never splits
/// a character.
pub fn error_preview(text: &str) -> ErrorPreview {
    let mut shown = display_line(text.chars().take(ERROR_PREVIEW_CHARS + 1));
    let cut = shown.chars().count() > ERROR_PREVIEW_CHARS;
    if cut {
        shown = shown.chars().take(ERROR_PREVIEW_CHARS).collect();
        shown.truncate(shown.trim_end().len());
        shown.push('…');
    }
    ErrorPreview { shown, cut }
}

/// The whole text as the details modal shows and copies it: each line with its control and
/// bidirectional characters as spaces, the line breaks kept.
fn details_text(text: &str) -> String {
    text.lines()
        .map(|line| display_line(line.chars()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The error details modal's content while it is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorDetails {
    pub title: String,
    /// The whole failure text as the modal shows it and Copy error copies it.
    pub text: String,
    /// "Copied" shows until this instant.
    pub copied_until: Option<DateTime<Utc>>,
    /// The toast card the modal opened from, while that card is in the stack.
    pub toast: Option<u64>,
}

impl AppState {
    /// The error details modal's content while it is open.
    pub fn details(&self) -> Option<&ErrorDetails> {
        self.details.as_ref()
    }

    /// Opens the details modal on `text` and closes the pill popover. A `toast` that names a
    /// card still in the stack holds new toasts until the modal closes. Details shown over open
    /// ones keep the card the modal first opened from, the card it hands focus back to.
    pub(super) fn show_details(&mut self, title: String, text: &str, toast: Option<u64>) {
        let toast = match &self.details {
            Some(open) => open.toast,
            None => toast.filter(|id| self.toasts().entries().iter().any(|e| e.id == *id)),
        };
        debug!(title = %title, chars = text.chars().count(), ?toast, "showing error details");
        self.pill_open = false;
        self.details = Some(ErrorDetails {
            title,
            text: details_text(text),
            copied_until: None,
            toast,
        });
    }

    pub(super) fn close_details(&mut self, out: &mut Outcome) {
        if self.details.take().is_none() {
            out.changed = false;
        }
    }

    pub(super) fn details_copied(&mut self, now: DateTime<Utc>, out: &mut Outcome) {
        match &mut self.details {
            Some(details) => details.copied_until = Some(now + FEEDBACK_FOR),
            None => out.changed = false,
        }
    }

    /// Ends the "Copied" label once its time is up; true when that changed what shows.
    pub(super) fn tick_details(&mut self, now: DateTime<Utc>) -> bool {
        let Some(details) = &mut self.details else {
            return false;
        };
        let due = details.copied_until.is_some_and(|until| now >= until);
        if due {
            details.copied_until = None;
        }
        due
    }
}

#[cfg(test)]
mod tests;
