//! The list's flow: arrived rows landing on screen, the quick search, the tail, and the
//! selection.

use super::rows::RowSummary;
use super::search::{
    Landing, Matcher, RebuildRequest, Rebuilt, SearchState, UI_MATCH_BYTES, snapshot,
};
use super::{ReadEffect, ReadOutcome, ReadState, Selected, Step};
use crate::state::ToastKind;
use rune::TextEdit;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tracing::{debug, trace, warn};

impl ReadState {
    /// The list's visible rows changed: `end` is one past the last visible position of
    /// `count`. Returns whether the pill's visibility flipped.
    pub fn visible_rows(&mut self, end: usize, count: usize) -> bool {
        let before = self.tail.pill_shows();
        self.tail.rows_from_bottom = count.saturating_sub(end);
        if self.tail.rows_from_bottom == 0 {
            self.tail.reset_unseen();
        }
        before != self.tail.pill_shows()
    }

    /// How many rows the list shows: every displayed row, or the search's matches.
    pub fn list_len(&self) -> usize {
        match &self.search {
            SearchState::Off => self.rows.displayed().len(),
            SearchState::Active { visible, .. } => visible.len(),
        }
    }

    /// The row at list position `position`.
    pub fn list_row(&self, position: usize) -> Option<&RowSummary> {
        match &self.search {
            SearchState::Off => self.rows.displayed().get(position),
            SearchState::Active { visible, .. } => self.rows.by_index(*visible.get(position)?),
        }
    }

    /// Moves arrived rows onto the screen and matches them against an active search.
    pub(super) fn flush(&mut self) -> ReadOutcome {
        let moved = self.rows.flush();
        if moved == 0 {
            return ReadOutcome::default();
        }
        let mut out = ReadOutcome {
            rows_landed: true,
            ..ReadOutcome::changed()
        };
        let displayed = self.rows.displayed();
        let arrived: Vec<&RowSummary> = displayed.range(displayed.len() - moved..).collect();
        let counted = match &mut self.search {
            SearchState::Off => arrived.len(),
            SearchState::Active {
                rebuilding: Some(_),
                ..
            } => 0,
            SearchState::Active {
                last_good,
                visible,
                rebuilding: rebuilding @ None,
            } => {
                let bytes: usize = arrived.iter().map(|row| row.message.body.len()).sum();
                if bytes > UI_MATCH_BYTES {
                    let current = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
                    *rebuilding = Some(current);
                    out.effects.push(ReadEffect::RebuildSearch(RebuildRequest {
                        generation: Arc::clone(&self.generation),
                        current,
                        matcher: last_good.clone(),
                        rows: snapshot(&self.rows),
                        upto: self.rows.last_displayed(),
                        follow_up: false,
                    }));
                    0
                } else {
                    let before = visible.len();
                    visible.extend(
                        arrived
                            .iter()
                            .filter(|row| last_good.matches(&row.message))
                            .map(|row| row.index),
                    );
                    visible.len() - before
                }
            }
        };
        if self.tail.rows_from_bottom > 0 {
            self.tail.unseen += counted as u64;
        }
        trace!(
            moved,
            evicted = self.rows.dropped,
            matched = counted,
            "list flush"
        );
        out
    }

    /// The search field or its regex toggle changed. An invalid pattern only shows its error:
    /// the filter and any rebuild in flight stay as they were.
    pub(super) fn search_edited(&mut self) -> ReadOutcome {
        let matcher = match Matcher::build(&self.search_edit.text, self.search_regex) {
            Err(error) => {
                self.search_error = Some(error);
                return ReadOutcome::changed();
            }
            Ok(None) => {
                self.search_error = None;
                self.clear_search();
                return ReadOutcome::changed();
            }
            Ok(Some(matcher)) => matcher,
        };
        self.search_error = None;
        let current = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let visible = match std::mem::take(&mut self.search) {
            SearchState::Active { visible, .. } => visible,
            SearchState::Off => {
                self.tail.reset_unseen();
                self.rows.epoch += 1;
                self.rows.displayed().iter().map(|row| row.index).collect()
            }
        };
        self.search = SearchState::Active {
            last_good: matcher.clone(),
            visible,
            rebuilding: Some(current),
        };
        ReadOutcome {
            effects: vec![ReadEffect::RebuildSearch(RebuildRequest {
                generation: Arc::clone(&self.generation),
                current,
                matcher,
                rows: snapshot(&self.rows),
                upto: self.rows.last_displayed(),
                follow_up: false,
            })],
            ..ReadOutcome::changed()
        }
    }

    /// Escape or a cleared field: the field empties and the list shows every row.
    pub(super) fn search_cleared(&mut self) -> ReadOutcome {
        self.search_edit = TextEdit::new("");
        self.search_error = None;
        self.clear_search();
        ReadOutcome::changed()
    }

    /// Turns the search off; a rebuild in flight then lands nowhere.
    pub(super) fn clear_search(&mut self) {
        let was_active = match self.search {
            SearchState::Off => false,
            SearchState::Active { .. } => true,
        };
        self.search = SearchState::Off;
        if was_active {
            self.generation.fetch_add(1, Ordering::AcqRel);
            self.rows.epoch += 1;
            self.tail.reset_unseen();
        }
    }

    pub(super) fn rebuilt(&mut self, rebuilt: Rebuilt) -> ReadOutcome {
        let generation = rebuilt.generation;
        let mode = rebuilt.matcher.mode();
        let (scanned, elapsed_ms) = (rebuilt.scanned, rebuilt.elapsed_ms);
        let current = self.generation.load(Ordering::Acquire);
        match self
            .search
            .land(rebuilt, current, &self.rows, &self.generation)
        {
            Landing::Dropped => {
                trace!(generation, "a stale quick search result was dropped");
                ReadOutcome::default()
            }
            Landing::Landed => {
                debug!(
                    generation,
                    scanned,
                    elapsed_ms,
                    matches = self.list_len(),
                    mode,
                    "quick search rebuilt"
                );
                self.landed()
            }
            Landing::Failed(error) => {
                warn!(generation, error = %error, "quick search rebuild failed; showing all rows");
                ReadOutcome {
                    toasts: vec![(
                        ToastKind::Danger,
                        "The filter stopped; the list shows every row".to_owned(),
                    )],
                    ..self.landed()
                }
            }
            Landing::FollowUp(request) => {
                debug!(
                    generation = request.current,
                    rows = request.rows.len(),
                    "quick search catching up with new rows"
                );
                ReadOutcome {
                    effects: vec![ReadEffect::RebuildSearch(request)],
                    ..self.landed()
                }
            }
        }
    }

    fn landed(&mut self) -> ReadOutcome {
        self.rows.epoch += 1;
        self.tail.reset_unseen();
        ReadOutcome::changed()
    }

    pub(super) fn select(&mut self, key_str: &str) -> ReadOutcome {
        let Some(row) = self.rows.by_key(key_str) else {
            debug!(key = %key_str, "a pressed row is no longer in the list");
            return ReadOutcome::default();
        };
        self.selected = Some(Selected {
            key: row.message.key.clone(),
            key_str: row.key_str.clone(),
            message: Arc::clone(&row.message),
        });
        self.inspector.set_open(true);
        ReadOutcome::changed()
    }

    pub(super) fn clear_selection(&mut self) -> ReadOutcome {
        self.selected = None;
        self.inspector.set_open(false);
        ReadOutcome::changed()
    }

    fn position_of_selection(&self) -> Option<usize> {
        let selected = self.selected.as_ref()?;
        (0..self.list_len()).find(|position| {
            self.list_row(*position)
                .is_some_and(|row| row.key_str == selected.key_str)
        })
    }

    /// Up, Down, Home, or End in the list. With nothing selected, Up and Down select the
    /// newest row.
    pub(super) fn step(&mut self, step: Step) -> ReadOutcome {
        let Some(last) = self.list_len().checked_sub(1) else {
            return ReadOutcome::default();
        };
        let target = match (step, self.position_of_selection()) {
            (Step::First, _) => 0,
            (Step::Last, _) | (Step::Up | Step::Down, None) => last,
            (Step::Up, Some(position)) => position.saturating_sub(1),
            (Step::Down, Some(position)) => (position + 1).min(last),
        };
        match self.list_row(target).map(|row| row.key_str.clone()) {
            Some(key_str) => self.select(&key_str),
            None => ReadOutcome::default(),
        }
    }
}
