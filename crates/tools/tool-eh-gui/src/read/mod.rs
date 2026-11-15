//! The Read section's model: the rows a session captured, the quick search over them, the
//! selected message, the capture rules, and the live statistics. Plain Rust, no window: the app
//! state forwards Read inputs here and runs the effects that come back.

pub mod export_flow;
pub mod inspector;
pub(crate) mod list_flow;
pub mod rows;
pub mod rules;
pub mod search;

pub use inspector::ExportOneRequest;

use crate::bridge::EngineSession;
use crate::state::{ConnectionState, ListSection, ToastKind};
use chrono::{DateTime, TimeDelta, Utc};
use export_flow::InspectorContext;
use inspector::{InspectorInput, InspectorState};
use rows::{RowBuffer, RowSource, RowSummary};
use rules::RulesForm;
use rune::TextEdit;
use search::{RebuildRequest, Rebuilt, SearchState};
use shared_eventhub_core::filters::CaptureFilters;
use shared_eventhub_core::models::{CapturedMessage, MessageKey};
use shared_eventhub_core::stats::StatsSnapshot;
use shared_eventhub_persistence::export::ExportFormat;
use shared_eventhub_persistence::paths::DataPaths;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tracing::{debug, info, warn};

/// The session log keeps this many lines.
pub const SESSION_LOG_CAP: usize = 200;
/// A connected hub with no message for this long is quiet.
pub const QUIET_AFTER: TimeDelta = TimeDelta::seconds(30);
/// No delivery for this many feedback intervals means the engine's feed stopped.
const STALE_INTERVALS: u32 = 3;
/// The toast for a file picked after the session that showed its message ended.
const DROPPED_ANSWER: &str = "The session ended before the export started; nothing was written";

/// The selected message.
#[derive(Clone, Debug)]
pub struct Selected {
    pub key: MessageKey,
    pub key_str: String,
    pub message: Arc<CapturedMessage>,
}

/// Where the list's viewport sits relative to its end, and what arrived out of view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TailState {
    /// Rows below the viewport's last visible row; 0 at the bottom.
    pub rows_from_bottom: usize,
    /// Rows that arrived while not at the bottom.
    pub unseen: u64,
    /// `unseen` as of the last tick, for the pill.
    pub shown_unseen: u64,
}

impl TailState {
    /// Whether the new-messages pill shows.
    pub fn pill_shows(&self) -> bool {
        self.shown_unseen > 0 && self.rows_from_bottom >= 2
    }

    fn reset_unseen(&mut self) {
        self.unseen = 0;
        self.shown_unseen = 0;
    }
}

/// What a session-log line reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogKind {
    Error,
    PartitionEnded,
}

/// One line of the session log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    pub at: DateTime<Utc>,
    pub kind: LogKind,
    pub partition: Option<String>,
    pub text: String,
}

/// The statistics the strip shows, as of the last tick.
#[derive(Clone, Debug, PartialEq)]
pub struct ShownStats {
    pub stats: StatsSnapshot,
    pub at: DateTime<Utc>,
    /// Connected and no message for [`QUIET_AFTER`].
    pub quiet: bool,
}

/// A list key move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Up,
    Down,
    First,
    Last,
}

/// Something that happened in the Read section.
#[derive(Debug)]
pub enum ReadInput {
    /// Move arrived rows onto the screen (every 250 ms).
    ListFlush,
    /// The search field or its regex toggle changed.
    SearchEdited,
    SearchCleared,
    SearchRebuilt(Rebuilt),
    Select(String),
    Move(Step),
    ClearSelection,
    Inspector(InspectorInput),
    ApplyRules,
    ClearRules,
}

/// Work the Read section asks for.
#[derive(Debug)]
pub enum ReadEffect {
    RebuildSearch(RebuildRequest),
    /// Send the filters to the running session.
    UpdateFilters(CaptureFilters),
    ExportOne(ExportOneRequest),
}

/// What a Read transition did.
#[derive(Debug, Default)]
pub struct ReadOutcome {
    pub effects: Vec<ReadEffect>,
    pub changed: bool,
    /// Rows moved onto the screen; their commit lands without motion.
    pub rows_landed: bool,
    pub toasts: Vec<(ToastKind, String)>,
}

impl ReadOutcome {
    fn changed() -> Self {
        Self {
            changed: true,
            ..Self::default()
        }
    }
}

/// The Read section's state.
pub struct ReadState {
    pub rules: RulesForm,
    pub rules_applied: CaptureFilters,
    pub stats: Option<StatsSnapshot>,
    pub shown: Option<ShownStats>,
    pub last_delivery_at: Option<DateTime<Utc>>,
    pub feed_stale: bool,
    pub feedback_interval: Duration,
    pub rows: RowBuffer,
    pub search: SearchState,
    pub search_edit: TextEdit,
    pub search_regex: bool,
    pub search_error: Option<String>,
    /// Shared with rebuilds in flight; never reset, so a result from an earlier search or
    /// session can never match the current generation.
    pub generation: Arc<AtomicU64>,
    pub tail: TailState,
    pub session_log: VecDeque<LogLine>,
    pub session_log_unseen: u32,
    pub inspector: InspectorState,
    /// Whether the Received tile's per-partition popover shows.
    pub partitions_open: bool,
    /// The session these rows belong to.
    session: Option<u64>,
    eviction_noted: bool,
}

impl std::fmt::Debug for ReadState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadState")
            .field("session", &self.session)
            .field("rows", &self.rows.displayed().len())
            .finish_non_exhaustive()
    }
}

impl Default for ReadState {
    fn default() -> Self {
        Self {
            rules: RulesForm::default(),
            rules_applied: CaptureFilters::default(),
            stats: None,
            shown: None,
            last_delivery_at: None,
            feed_stale: false,
            feedback_interval: Duration::from_secs(1),
            rows: RowBuffer::default(),
            search: SearchState::Off,
            search_edit: TextEdit::new(""),
            search_regex: false,
            search_error: None,
            generation: Arc::new(AtomicU64::new(0)),
            tail: TailState::default(),
            session_log: VecDeque::new(),
            session_log_unseen: 0,
            inspector: InspectorState::default(),
            partitions_open: false,
            session: None,
            eviction_noted: false,
        }
    }
}

impl ReadState {
    /// A Read section seeded from a recalled config's rules and export settings.
    pub fn with_rules(filters: &CaptureFilters, format: ExportFormat, metadata: bool) -> Self {
        Self {
            rules: RulesForm::from_filters(filters),
            rules_applied: filters.clone(),
            inspector: InspectorState::with_export(format, metadata),
            ..Self::default()
        }
    }

    /// Starts over for `session`, keeping the rules, the inspector's picks, width, and an
    /// export still writing, and the search generation (moved on, so no earlier result lands).
    pub fn start_session(&mut self, session: u64, feedback_interval: Duration) {
        let previous = std::mem::take(self);
        previous.generation.fetch_add(1, Ordering::AcqRel);
        let mut inspector = previous.inspector;
        inspector.reset();
        *self = Self {
            rules: previous.rules,
            rules_applied: previous.rules_applied,
            feedback_interval,
            generation: previous.generation,
            inspector,
            session: Some(session),
            ..Self::default()
        };
    }

    /// Drops what only a running session shows; the session log stays for the Failed state.
    pub fn end_session(&mut self) {
        info!(
            session = ?self.session,
            rows_evicted = self.rows.dropped,
            "the read list closed with the session"
        );
        self.rows = RowBuffer::default();
        self.search_edit = TextEdit::new("");
        self.search_regex = false;
        self.search_error = None;
        self.clear_search();
        self.inspector.reset();
        self.tail = TailState::default();
        self.stats = None;
        self.shown = None;
        self.last_delivery_at = None;
        self.feed_stale = false;
        self.partitions_open = false;
    }

    /// Whether an export still writes a file (the app must not close the archive under it).
    pub fn export_running(&self) -> bool {
        self.inspector.export.is_running()
    }

    /// Records a delivery's arrival and statistics.
    pub fn delivered(&mut self, at: DateTime<Utc>, stats: Option<StatsSnapshot>) {
        self.last_delivery_at = Some(at);
        if let Some(stats) = stats {
            self.stats = Some(stats);
        }
    }

    /// A captured message: it waits for the next flush.
    pub fn message(&mut self, message: CapturedMessage) {
        let pushed = self.rows.push(Arc::new(message));
        if pushed.evicted_displayed > 0 {
            self.search.prune(self.rows.base_index());
        }
        if let Some(trigger) = pushed.trigger
            && !self.eviction_noted
        {
            self.eviction_noted = true;
            info!(
                session = ?self.session,
                trigger = trigger.as_str(),
                row_cap = rows::ROW_CAP,
                bytes_cap_mib = rows::ROW_BODY_BYTES_CAP / (1024 * 1024),
                "the read list is full; older rows now leave it"
            );
        }
    }

    /// A line for the session log; counted as unseen unless its popover is open.
    pub fn log(&mut self, line: LogLine, popover_open: bool) {
        if self.session_log.len() == SESSION_LOG_CAP {
            self.session_log.pop_front();
        }
        self.session_log.push_back(line);
        if !popover_open {
            self.session_log_unseen = self.session_log_unseen.saturating_add(1);
        }
    }

    /// The pill's popover opened: every log line has been seen.
    pub fn log_seen(&mut self) {
        self.session_log_unseen = 0;
    }

    /// The once-a-second refresh: numbers, staleness, the pill count, and feedback expiry.
    /// Returns whether anything shown changed.
    pub fn tick(&mut self, now: DateTime<Utc>, connected: bool) -> bool {
        let mut changed = self.inspector.tick(now);
        if self.tail.shown_unseen != self.tail.unseen {
            self.tail.shown_unseen = self.tail.unseen;
            changed = true;
        }
        if !connected {
            return changed;
        }
        if let Some(stats) = &self.stats {
            let since = stats.last_message_at.unwrap_or(stats.started_at);
            let shown = ShownStats {
                stats: stats.clone(),
                at: now,
                quiet: now - since > QUIET_AFTER,
            };
            if self.shown.as_ref().is_some_and(|s| s.quiet) != shown.quiet {
                debug!(session = ?self.session, quiet = shown.quiet, "the hub's quiet state changed");
            }
            changed |= self.shown.as_ref() != Some(&shown);
            self.shown = Some(shown);
        }
        changed | self.check_feed(now)
    }

    /// The 3 x feedback-interval rule, logged on each transition.
    fn check_feed(&mut self, now: DateTime<Utc>) -> bool {
        let Some(last) = self.last_delivery_at else {
            return false;
        };
        let limit =
            TimeDelta::from_std(self.feedback_interval * STALE_INTERVALS).unwrap_or(TimeDelta::MAX);
        let silent = now - last;
        let stale = silent > limit;
        if stale == self.feed_stale {
            return false;
        }
        self.feed_stale = stale;
        if stale {
            warn!(
                session = ?self.session,
                silent_s = silent.num_seconds(),
                feedback_interval_s = self.feedback_interval.as_secs(),
                "no engine updates for 3x the feedback interval"
            );
        } else {
            info!(
                session = ?self.session,
                stale_ms = silent.num_milliseconds(),
                "engine updates resumed"
            );
        }
        true
    }

    /// Applies `input`. `connection` decides where rules go and which session an export
    /// belongs to; `paths` is the data folder an export reads.
    pub fn apply(
        &mut self,
        input: ReadInput,
        connection: &ConnectionState,
        paths: DataPaths,
    ) -> ReadOutcome {
        match input {
            ReadInput::ListFlush => self.flush(),
            ReadInput::SearchEdited => self.search_edited(),
            ReadInput::SearchCleared => self.search_cleared(),
            ReadInput::SearchRebuilt(rebuilt) => self.rebuilt(rebuilt),
            ReadInput::Select(key_str) => self.select(&key_str),
            ReadInput::Move(step) => self.step(step),
            ReadInput::ClearSelection => self.clear_selection(),
            ReadInput::Inspector(input) => {
                let cx = InspectorContext {
                    section: ListSection::Read,
                    owner: connection.session().map(EngineSession::id),
                    paths: &paths,
                    dropped_answer: DROPPED_ANSWER,
                };
                let out = self.inspector.apply(input, cx);
                ReadOutcome {
                    effects: out.export.into_iter().map(ReadEffect::ExportOne).collect(),
                    changed: out.changed,
                    rows_landed: false,
                    toasts: out.toasts,
                }
            }
            ReadInput::ApplyRules => match self.rules.parsed() {
                Ok(filters) => self.apply_rules(filters, connection),
                Err(_) => ReadOutcome::default(),
            },
            ReadInput::ClearRules => {
                let out = self.apply_rules(CaptureFilters::default(), connection);
                if out.changed {
                    self.rules = RulesForm::default();
                }
                out
            }
        }
    }

    fn apply_rules(
        &mut self,
        filters: CaptureFilters,
        connection: &ConnectionState,
    ) -> ReadOutcome {
        let rules = rules::rule_count(&filters);
        match connection {
            ConnectionState::Connected { session, .. }
            | ConnectionState::Connecting {
                session: Some(session),
                ..
            } => {
                debug!(
                    session = session.id(),
                    rules, "capture rules sent to the engine"
                );
                self.rules_applied = filters.clone();
                ReadOutcome {
                    effects: vec![ReadEffect::UpdateFilters(filters)],
                    ..ReadOutcome::changed()
                }
            }
            ConnectionState::Disconnected
            | ConnectionState::Failed(_)
            | ConnectionState::Stopping { .. } => {
                debug!(rules, "capture rules kept for the next connect");
                self.rules_applied = filters;
                ReadOutcome::changed()
            }
            ConnectionState::Connecting { session: None, .. } => {
                debug!("capture rules refused while the session is being prepared");
                ReadOutcome::default()
            }
        }
    }
}

impl RowSource for ReadState {
    fn count(&self) -> usize {
        self.list_len()
    }

    fn row(&self, position: usize) -> Option<&RowSummary> {
        self.list_row(position)
    }

    fn epoch(&self) -> u64 {
        self.rows.epoch
    }
}

/// Whether rules can change now: refused only while the session's request is being built.
pub fn rules_editable(connection: &ConnectionState) -> bool {
    match connection {
        ConnectionState::Connecting { session: None, .. } => false,
        ConnectionState::Disconnected
        | ConnectionState::Connecting {
            session: Some(_), ..
        }
        | ConnectionState::Connected { .. }
        | ConnectionState::Stopping { .. }
        | ConnectionState::Failed(_) => true,
    }
}

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
