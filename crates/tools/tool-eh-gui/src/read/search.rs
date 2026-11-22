//! The Read list's quick search: an ephemeral view filter over the rows on screen.
//!
//! It matches with phase 2's `contains_any_ci`, the same primitive capture filters use, so the
//! two cannot drift on what "contains" means. It does not reuse `CaptureFilters` itself: capture
//! filters decide what a run keeps and emits, this decides which rows the list shows, and it
//! also looks at the message and correlation ids.

use crate::read::rows::RowBuffer;
use regex::{Regex, RegexBuilder};
use shared_eventhub_core::filters::contains_any_ci;
use shared_eventhub_core::models::CapturedMessage;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// The longest pattern the search compiles.
pub const MAX_PATTERN_CHARS: usize = 1_000;
/// The compiled-program limit for a search regex; a pattern past it is refused.
const REGEX_SIZE_LIMIT: usize = 1 << 20;
/// How many body bytes a flush or a landing rebuild matches on the UI thread before it hands
/// the rest to a rebuild.
pub const UI_MATCH_BYTES: usize = 4 * 1024 * 1024;
/// How long a rebuild waits for typing to pause.
pub const REBUILD_DEBOUNCE: Duration = Duration::from_millis(150);
/// How many rows a scan matches between checks that its search is still current.
const SCAN_CHECK_EVERY: usize = 64;

/// What a search matches.
#[derive(Clone, Debug)]
pub enum Matcher {
    Contains(Vec<String>),
    Regex(Regex),
}

impl Matcher {
    /// The matcher for `text`: `None` for blank text (no search).
    ///
    /// # Errors
    /// A pattern past [`MAX_PATTERN_CHARS`], or a regex that does not compile, returns the text
    /// shown under the field.
    pub fn build(text: &str, regex: bool) -> Result<Option<Self>, String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        if text.chars().count() > MAX_PATTERN_CHARS {
            return Err(format!(
                "Search text is limited to {MAX_PATTERN_CHARS} characters"
            ));
        }
        if !regex {
            return Ok(Some(Self::Contains(vec![text.to_owned()])));
        }
        RegexBuilder::new(text)
            .case_insensitive(true)
            .size_limit(REGEX_SIZE_LIMIT)
            .build()
            .map(|compiled| Some(Self::Regex(compiled)))
            .map_err(|error| format!("Invalid regular expression: {error}"))
    }

    /// Whether `message`'s body, message id, or correlation id matches.
    pub fn matches(&self, message: &CapturedMessage) -> bool {
        let ids = [&message.message_id, &message.correlation_id];
        match self {
            Self::Contains(needles) => {
                contains_any_ci(&message.body, needles)
                    || ids
                        .into_iter()
                        .flatten()
                        .any(|id| contains_any_ci(id, needles))
            }
            Self::Regex(regex) => {
                regex.is_match(&message.body)
                    || ids.into_iter().flatten().any(|id| regex.is_match(id))
            }
        }
    }

    pub fn mode(&self) -> &'static str {
        match self {
            Self::Contains(_) => "contains",
            Self::Regex(_) => "regex",
        }
    }
}

/// Whether the list is filtered, and by what.
#[derive(Debug, Default)]
pub enum SearchState {
    #[default]
    Off,
    Active {
        /// The last valid matcher: `visible` was built with it and new rows match it.
        last_good: Matcher,
        /// Global indices of the displayed rows that match, ascending.
        visible: Vec<u64>,
        /// The generation of the rebuild in flight.
        rebuilding: Option<u64>,
    },
}

/// A rebuild to run off the UI thread: match `rows` with `matcher` unless `generation` moves on
/// from `current`.
#[derive(Debug)]
pub struct RebuildRequest {
    pub generation: Arc<AtomicU64>,
    pub current: u64,
    pub matcher: Matcher,
    pub rows: Vec<(u64, Arc<CapturedMessage>)>,
    /// The newest index in `rows`; rows after it arrived later.
    pub upto: Option<u64>,
    /// A follow-up appends to `visible` instead of replacing it.
    pub follow_up: bool,
}

impl RebuildRequest {
    /// Whether the search this request belongs to is still the current one.
    pub fn is_current(&self) -> bool {
        self.generation.load(Ordering::Acquire) == self.current
    }

    /// The matching indices, or `None` when the search moved on mid-scan.
    pub fn scan(&self) -> Option<Vec<u64>> {
        let mut matches = Vec::new();
        for (scanned, (index, message)) in self.rows.iter().enumerate() {
            if scanned % SCAN_CHECK_EVERY == 0 && !self.is_current() {
                return None;
            }
            if self.matcher.matches(message) {
                matches.push(*index);
            }
        }
        Some(matches)
    }
}

/// A rebuild's result, back on the UI thread.
#[derive(Debug)]
pub struct Rebuilt {
    pub generation: u64,
    pub matcher: Matcher,
    pub result: Result<Vec<u64>, String>,
    pub upto: Option<u64>,
    pub follow_up: bool,
    /// Rows the scan looked at, and how long it took.
    pub scanned: usize,
    pub elapsed_ms: u128,
}

/// What landing a rebuild did.
#[derive(Debug)]
pub enum Landing {
    /// Not the current rebuild; nothing changed.
    Dropped,
    /// `visible` changed.
    Landed,
    /// The scan failed; the search is off and the list shows every row.
    Failed(String),
    /// `visible` holds the result up to `upto`; more rows arrived than the UI thread matches,
    /// so this rebuild covers them.
    FollowUp(RebuildRequest),
}

impl SearchState {
    /// The rebuild in flight, if any.
    pub fn rebuilding(&self) -> Option<u64> {
        match self {
            Self::Off => None,
            Self::Active { rebuilding, .. } => *rebuilding,
        }
    }

    /// Drops matches that left the buffer.
    pub fn prune(&mut self, base_index: u64) {
        match self {
            Self::Off => {}
            Self::Active { visible, .. } => visible.retain(|index| *index >= base_index),
        }
    }

    /// Lands `rebuilt` when it is the rebuild in flight for `current`.
    pub fn land(
        &mut self,
        rebuilt: Rebuilt,
        current: u64,
        rows: &RowBuffer,
        generation: &Arc<AtomicU64>,
    ) -> Landing {
        let (visible, rebuilding) = match self {
            Self::Off => return Landing::Dropped,
            Self::Active {
                visible,
                rebuilding,
                ..
            } => (visible, rebuilding),
        };
        if rebuilt.generation != current || *rebuilding != Some(rebuilt.generation) {
            return Landing::Dropped;
        }
        let mut matches = match rebuilt.result {
            Ok(matches) => matches,
            Err(error) => {
                // The filter can no longer be trusted to cover the rows on screen, so it ends;
                // a rebuild in flight then finds no search to land on.
                *self = Self::Off;
                generation.fetch_add(1, Ordering::AcqRel);
                return Landing::Failed(error);
            }
        };
        let base = rows.base_index();
        matches.retain(|index| *index >= base);
        if rebuilt.follow_up {
            visible.retain(|index| *index >= base);
            visible.extend(matches);
        } else {
            *visible = matches;
        }

        let late = later_rows(rows, rebuilt.upto);
        let late_bytes: usize = late.iter().map(|(_, message)| message.body.len()).sum();
        if late_bytes > UI_MATCH_BYTES {
            let next = generation.fetch_add(1, Ordering::AcqRel) + 1;
            *rebuilding = Some(next);
            return Landing::FollowUp(RebuildRequest {
                generation: Arc::clone(generation),
                current: next,
                matcher: rebuilt.matcher,
                upto: rows.last_displayed(),
                rows: late,
                follow_up: true,
            });
        }
        visible.extend(
            late.iter()
                .filter(|(_, message)| rebuilt.matcher.matches(message))
                .map(|(index, _)| *index),
        );
        *rebuilding = None;
        Landing::Landed
    }
}

/// The displayed rows after `upto` (all of them when `upto` is `None`).
fn later_rows(rows: &RowBuffer, upto: Option<u64>) -> Vec<(u64, Arc<CapturedMessage>)> {
    rows.displayed()
        .iter()
        .filter(|row| upto.is_none_or(|upto| row.index > upto))
        .map(|row| (row.index, Arc::clone(&row.message)))
        .collect()
}

/// Every displayed row, for a rebuild's snapshot.
pub fn snapshot(rows: &RowBuffer) -> Vec<(u64, Arc<CapturedMessage>)> {
    later_rows(rows, None)
}

#[cfg(test)]
mod tests;
