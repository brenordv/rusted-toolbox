//! The Read list's rows: messages that arrived since the last flush, the rows on screen, and
//! the caps that bound both.

use shared_eventhub_core::models::{CapturedMessage, MessageKey};
use std::collections::VecDeque;
use std::sync::Arc;

/// The most rows held, arrived and displayed together.
pub const ROW_CAP: usize = 5_000;
/// The most body bytes held, arrived and displayed together. At Event Hubs' 1 MB message limit,
/// [`ROW_CAP`] rows alone could hold 5 GB; this keeps the list's memory bounded whatever the
/// payloads.
pub const ROW_BODY_BYTES_CAP: usize = 256 * 1024 * 1024;
/// How much of a body the list's preview column shows.
pub const PREVIEW_CHARS: usize = 160;

/// One message in the list.
#[derive(Clone, Debug)]
pub struct RowSummary {
    /// Stable for the session: rows keep their index while older rows leave.
    pub index: u64,
    /// The row's key in the list, from its message key.
    pub key_str: String,
    pub message: Arc<CapturedMessage>,
    /// The start of the body on one line, safe to display.
    pub preview: String,
}

impl RowSummary {
    pub(crate) fn new(index: u64, message: Arc<CapturedMessage>) -> Self {
        Self {
            index,
            key_str: key_string(&message.key),
            preview: display_line(message.body.chars().take(PREVIEW_CHARS)),
            message,
        }
    }
}

/// What a message list shows: its rows by position, and a number that changes whenever the
/// sequence of row keys changes at an unchanged count.
pub trait RowSource {
    fn count(&self) -> usize;
    fn row(&self, position: usize) -> Option<&RowSummary>;
    fn epoch(&self) -> u64;
}

/// The list key of a message: `p{partition}-s{sequence}`. The only message identifier logs use.
pub fn key_string(key: &MessageKey) -> String {
    format!("p{}-s{}", key.partition_id, key.sequence_number)
}

/// Whether `c` must not reach a one-line display: control characters, line and paragraph
/// separators, and the bidirectional controls that can make shown text read differently from
/// what copy and search see.
fn is_unsafe_in_a_line(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{2028}'
                | '\u{2029}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2066}'..='\u{2069}'
                | '\u{200E}'
                | '\u{200F}'
                | '\u{061C}'
        )
}

/// `chars` on one line, with every character [`is_unsafe_in_a_line`] turned into a space.
pub fn display_line(chars: impl Iterator<Item = char>) -> String {
    chars
        .map(|c| if is_unsafe_in_a_line(c) { ' ' } else { c })
        .collect()
}

/// `text` with its first letter capitalized, for a parser message shown on its own.
pub fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The longest start of `text` that is at most `max` bytes and ends on a character boundary.
pub fn cut_at_char_boundary(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// What adding a message did to the rows on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pushed {
    /// Displayed rows that left to make room.
    pub evicted_displayed: usize,
    /// Arrived, not yet displayed rows that left to make room.
    pub evicted_incoming: usize,
    /// The cap that made room, when one did.
    pub trigger: Option<EvictTrigger>,
}

/// Which cap evicted rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvictTrigger {
    Rows,
    Bytes,
}

impl EvictTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rows => "rows",
            Self::Bytes => "bytes",
        }
    }
}

/// Rows that arrived since the last flush, then the rows on screen, oldest first. Indices run
/// contiguously from `base_index` through the displayed rows and on through the arrived ones.
#[derive(Debug)]
pub struct RowBuffer {
    incoming: VecDeque<RowSummary>,
    rows: VecDeque<RowSummary>,
    /// The index of the oldest row held.
    base_index: u64,
    next_index: u64,
    body_bytes: usize,
    row_cap: usize,
    bytes_cap: usize,
    /// Rows evicted this session.
    pub dropped: u64,
    /// Changes whenever the displayed key sequence changes at an unchanged count.
    pub epoch: u64,
}

impl Default for RowBuffer {
    fn default() -> Self {
        Self::with_caps(ROW_CAP, ROW_BODY_BYTES_CAP)
    }
}

impl RowBuffer {
    /// An empty buffer holding at most `row_cap` rows and `bytes_cap` body bytes.
    pub(crate) fn with_caps(row_cap: usize, bytes_cap: usize) -> Self {
        Self {
            incoming: VecDeque::new(),
            rows: VecDeque::new(),
            base_index: 0,
            next_index: 0,
            body_bytes: 0,
            row_cap,
            bytes_cap,
            dropped: 0,
            epoch: 0,
        }
    }

    /// Adds `message` as an arrived row, evicting the oldest rows first while a cap would be
    /// passed.
    pub fn push(&mut self, message: Arc<CapturedMessage>) -> Pushed {
        let bytes = message.body.len();
        let mut pushed = Pushed::default();
        loop {
            let held = self.rows.len() + self.incoming.len();
            let trigger = if held == 0 {
                break;
            } else if held >= self.row_cap {
                EvictTrigger::Rows
            } else if self.body_bytes + bytes > self.bytes_cap {
                EvictTrigger::Bytes
            } else {
                break;
            };
            pushed.trigger.get_or_insert(trigger);
            if self.evict_oldest() {
                pushed.evicted_displayed += 1;
            } else {
                pushed.evicted_incoming += 1;
            }
        }
        if pushed.evicted_displayed > 0 {
            self.epoch += 1;
        }
        let row = RowSummary::new(self.next_index, message);
        self.next_index += 1;
        self.body_bytes += bytes;
        self.incoming.push_back(row);
        pushed
    }

    /// Removes the oldest row held; true when it was a displayed one.
    fn evict_oldest(&mut self) -> bool {
        let (row, displayed) = match self.rows.pop_front() {
            Some(row) => (row, true),
            None => match self.incoming.pop_front() {
                Some(row) => (row, false),
                None => return false,
            },
        };
        self.base_index = row.index + 1;
        self.body_bytes -= row.message.body.len();
        self.dropped += 1;
        displayed
    }

    /// Moves every arrived row onto the screen; returns how many moved.
    pub fn flush(&mut self) -> usize {
        let moved = self.incoming.len();
        self.rows.append(&mut self.incoming);
        moved
    }

    /// The rows on screen, oldest first.
    pub fn displayed(&self) -> &VecDeque<RowSummary> {
        &self.rows
    }

    /// The displayed row with global index `index`.
    pub fn by_index(&self, index: u64) -> Option<&RowSummary> {
        let offset = usize::try_from(index.checked_sub(self.base_index)?).ok()?;
        self.rows.get(offset)
    }

    /// The displayed row keyed `key_str`.
    pub fn by_key(&self, key_str: &str) -> Option<&RowSummary> {
        self.rows.iter().find(|row| row.key_str == key_str)
    }

    /// The index of the oldest row held; displayed indices below it are gone.
    pub fn base_index(&self) -> u64 {
        self.base_index
    }

    /// The index of the newest displayed row.
    pub fn last_displayed(&self) -> Option<u64> {
        self.rows.back().map(|row| row.index)
    }

    pub fn incoming_len(&self) -> usize {
        self.incoming.len()
    }

    pub fn body_bytes(&self) -> usize {
        self.body_bytes
    }
}

#[cfg(test)]
mod tests;
