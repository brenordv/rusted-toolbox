//! The archive search's results: where loading them is, and the rows loaded so far.

use crate::read::rows::{ROW_BODY_BYTES_CAP, ROW_CAP, RowSummary};
use shared_eventhub_core::models::{CapturedMessage, MessageKey};
use shared_eventhub_persistence::search::SearchCursor;
use std::collections::HashSet;
use std::sync::Arc;

/// Messages per archive page.
pub const ARCHIVE_PAGE: usize = 250;
/// The next page loads once the last visible row is this close to the loaded end.
pub const LOAD_AHEAD: usize = 100;

/// Where the results of the current search are.
#[derive(Debug, Default)]
pub enum Results {
    #[default]
    NotSearched,
    /// The first page of search `generation` is being read.
    Loading { generation: u64 },
    /// The first page failed: the archive could not be opened or read.
    Unavailable { generation: u64, error: String },
    /// At least the first page landed.
    Shown {
        generation: u64,
        rows: ArchiveRows,
        more: More,
    },
}

/// What comes after the loaded rows.
#[derive(Debug)]
pub enum More {
    /// More matches exist; nothing is being read.
    Ready(SearchCursor),
    /// The page after `0` is being read.
    Loading(SearchCursor),
    /// Reading the page after `cursor` failed.
    Failed { cursor: SearchCursor, error: String },
    /// Every match is loaded.
    End,
    /// The loaded rows reached a cap; more matches may exist.
    Capped,
}

impl More {
    /// How the log names this state.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ready(_) => "ready",
            Self::Loading(_) => "loading",
            Self::Failed { .. } => "failed",
            Self::End => "end",
            Self::Capped => "capped",
        }
    }
}

/// The loaded rows, in archive order, with each message at most once.
#[derive(Debug)]
pub struct ArchiveRows {
    rows: Vec<RowSummary>,
    keys: HashSet<MessageKey>,
    body_bytes: usize,
    row_cap: usize,
    bytes_cap: usize,
}

impl Default for ArchiveRows {
    fn default() -> Self {
        Self::with_caps(ROW_CAP, ROW_BODY_BYTES_CAP)
    }
}

/// What appending a page did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Appended {
    pub added: usize,
    /// Messages already loaded from an earlier page.
    pub skipped: usize,
    /// A message did not fit under a cap.
    pub full: bool,
}

impl ArchiveRows {
    /// Empty rows holding at most `row_cap` rows and `bytes_cap` body bytes.
    pub(crate) fn with_caps(row_cap: usize, bytes_cap: usize) -> Self {
        Self {
            rows: Vec::new(),
            keys: HashSet::new(),
            body_bytes: 0,
            row_cap,
            bytes_cap,
        }
    }

    pub fn rows(&self) -> &[RowSummary] {
        &self.rows
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn body_bytes(&self) -> usize {
        self.body_bytes
    }

    /// Whether the row cap is reached.
    pub fn at_row_cap(&self) -> bool {
        self.rows.len() >= self.row_cap
    }

    pub fn by_key(&self, key_str: &str) -> Option<&RowSummary> {
        self.rows.iter().find(|row| row.key_str == key_str)
    }

    /// Appends `items` in order, skipping messages already held, until one does not fit under
    /// a cap.
    pub fn append(&mut self, items: Vec<CapturedMessage>) -> Appended {
        let mut appended = Appended::default();
        for message in items {
            if self.keys.contains(&message.key) {
                appended.skipped += 1;
                continue;
            }
            let bytes = message.body.len();
            if self.rows.len() >= self.row_cap || self.body_bytes + bytes > self.bytes_cap {
                appended.full = true;
                break;
            }
            self.keys.insert(message.key.clone());
            self.body_bytes += bytes;
            let position = self.rows.len() as u64;
            self.rows.push(RowSummary::new(position, Arc::new(message)));
            appended.added += 1;
        }
        appended
    }
}

/// What follows a page that `appended` landed with `next`.
pub fn more_after(rows: &ArchiveRows, appended: Appended, next: Option<SearchCursor>) -> More {
    if appended.full {
        return More::Capped;
    }
    match next {
        Some(_) if rows.at_row_cap() => More::Capped,
        Some(cursor) => More::Ready(cursor),
        None => More::End,
    }
}

#[cfg(test)]
mod tests;
