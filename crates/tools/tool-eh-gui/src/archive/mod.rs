//! The Export section's model: the archive search form, its results loaded page by page, the
//! selected message, and the bulk export of every match. Plain Rust, no window: the app state
//! forwards Export inputs here and runs the effects that come back.

pub mod bulk;
pub mod results;

use crate::read::export_flow::InspectorContext;
use crate::read::inspector::{ExportOneRequest, InspectorInput, InspectorState};
use crate::read::list_flow::{position_of, step_target};
use crate::read::rows::{RowSource, RowSummary};
use crate::read::rules::{RulesForm, rule_count};
use crate::read::{Selected, Step};
use crate::state::{Section, ToastKind};
use bulk::{BulkExport, BulkOptions, BulkRequest, CancelReason};
use chrono::{DateTime, Utc};
use results::{ARCHIVE_PAGE, ArchiveRows, LOAD_AHEAD, More, Results, more_after};
use shared_eventhub_core::filters::{CaptureFilters, TextFilter};
use shared_eventhub_persistence::config::ExportSettings;
use shared_eventhub_persistence::export::ExportOutcome;
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::search::{SearchCursor, SearchPage, SearchQuery};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// The toast for a file picked after the results that showed its message were replaced.
const DROPPED_ANSWER: &str = "The results changed before the export started; nothing was written";

/// Something that happened in the Export section.
#[derive(Debug)]
pub enum ArchiveInput {
    SearchPressed,
    /// Empty the search form; the results stay.
    ClearForm,
    PageLoaded {
        generation: u64,
        elapsed_ms: u128,
        result: Result<SearchPage, String>,
    },
    /// The list's visible rows end before position `end`.
    VisibleRows {
        end: usize,
    },
    RetryPage,
    Select(String),
    Move(Step),
    ClearSelection,
    Inspector(InspectorInput),
    BulkExportPressed,
    CancelBulk,
    DismissBulk,
    BulkProgress {
        export: u64,
        counts: ExportOutcome,
    },
    BulkDone {
        export: u64,
        elapsed_ms: u128,
        result: Result<ExportOutcome, String>,
    },
    DestinationPicked(PathBuf),
    DestinationFailed(String),
}

/// Work the Export section asks for.
#[derive(Debug)]
pub enum ArchiveEffect {
    Search(PageRequest),
    ExportOne(ExportOneRequest),
    BulkExport(BulkRequest),
}

/// Read one page of `query` from the archive of `paths`, for search `generation`.
#[derive(Debug)]
pub struct PageRequest {
    pub generation: u64,
    pub query: SearchQuery,
    pub paths: DataPaths,
}

/// What an Export transition did.
#[derive(Debug, Default)]
pub struct ArchiveOutcome {
    pub effects: Vec<ArchiveEffect>,
    pub changed: bool,
    pub toasts: Vec<(ToastKind, String)>,
    /// A bulk export ended in a failure the user must see.
    pub run_failed: bool,
}

impl ArchiveOutcome {
    fn changed() -> Self {
        Self {
            changed: true,
            ..Self::default()
        }
    }
}

/// The Export section's state.
#[derive(Debug, Default)]
pub struct ArchiveState {
    pub form: RulesForm,
    pub results: Results,
    pub inspector: InspectorState,
    pub bulk: BulkExport,
    pub options: BulkOptions,
    /// Whether the bulk options popover shows.
    pub options_open: bool,
    /// The query of the last search; a bulk export exports all of it.
    last_query: Option<SearchQuery>,
    /// The current search; never reset, so an older page never matches.
    generation: u64,
    /// The data folder's token for the inspector's exports; moves on with every folder.
    folder_epoch: u64,
    /// Moves on whenever the rows change, so the list never reuses a version.
    epoch: u64,
    /// One past the last visible list position, as last reported.
    visible_end: usize,
    /// Pages loaded for the current search, for the log.
    pages: u32,
    next_export: u64,
}

impl ArchiveState {
    /// An Export section seeded from a recalled config's export settings.
    pub fn with_settings(settings: &ExportSettings) -> Self {
        Self {
            inspector: InspectorState::with_export(settings.format, settings.include_metadata),
            options: BulkOptions::from_settings(settings),
            ..Self::default()
        }
    }

    /// Whether a page is being read (a new search waits for it).
    pub fn fetch_in_flight(&self) -> bool {
        match &self.results {
            Results::Loading { .. }
            | Results::Shown {
                more: More::Loading(_),
                ..
            } => true,
            Results::NotSearched
            | Results::Unavailable { .. }
            | Results::Shown {
                more: More::Ready(_) | More::Failed { .. } | More::End | More::Capped,
                ..
            } => false,
        }
    }

    /// Whether the Search button can run a search now.
    pub fn searchable(&self) -> bool {
        !self.fetch_in_flight() && self.form.parsed().is_ok()
    }

    /// Whether a bulk export or a message export still writes files.
    pub fn busy(&self) -> bool {
        self.bulk.is_running() || self.inspector.export.is_running()
    }

    /// Whether anything was searched since the data folder last changed.
    pub fn searched(&self) -> bool {
        self.last_query.is_some()
    }

    /// The window closed: a running bulk export is asked to stop.
    pub fn window_closed(&mut self) {
        self.cancel_bulk(CancelReason::WindowClosed);
    }

    /// The data folder changed: everything read from the old one goes.
    pub fn folder_changed(&mut self) {
        info!(
            generation = self.generation,
            "the archive search starts over for the new data folder"
        );
        self.generation += 1;
        self.folder_epoch += 1;
        self.epoch += 1;
        self.results = Results::NotSearched;
        self.last_query = None;
        self.visible_end = 0;
        self.inspector.reset();
        self.options.destination = None;
        match &self.bulk {
            BulkExport::Done { .. } => self.bulk = BulkExport::Idle,
            BulkExport::Idle | BulkExport::Running { .. } => {}
        }
    }

    /// Feedback whose time is up ends; true when something shown changed.
    pub fn tick(&mut self, now: DateTime<Utc>) -> bool {
        self.inspector.tick(now)
    }

    /// Applies `input`; `paths` is the current data folder.
    pub fn apply(&mut self, input: ArchiveInput, paths: &DataPaths) -> ArchiveOutcome {
        match input {
            ArchiveInput::SearchPressed => self.search_pressed(paths),
            ArchiveInput::ClearForm => {
                self.form = RulesForm::default();
                ArchiveOutcome::changed()
            }
            ArchiveInput::PageLoaded {
                generation,
                elapsed_ms,
                result,
            } => self.page_loaded(generation, elapsed_ms, result, paths),
            ArchiveInput::VisibleRows { end } => {
                self.visible_end = end;
                self.fetch_if_near_end(paths)
            }
            ArchiveInput::RetryPage => self.retry_page(paths),
            ArchiveInput::Select(key_str) => self.select(&key_str),
            ArchiveInput::Move(step) => self.step(step),
            ArchiveInput::ClearSelection => {
                self.inspector.clear_selection();
                ArchiveOutcome::changed()
            }
            ArchiveInput::Inspector(input) => self.inspector_input(input, paths),
            ArchiveInput::BulkExportPressed => self.bulk_pressed(paths),
            ArchiveInput::CancelBulk => ArchiveOutcome {
                changed: self.cancel_bulk(CancelReason::Button),
                ..ArchiveOutcome::default()
            },
            ArchiveInput::DismissBulk => self.dismiss_bulk(),
            ArchiveInput::BulkProgress { export, counts } => self.bulk_progress(export, counts),
            ArchiveInput::BulkDone {
                export,
                elapsed_ms,
                result,
            } => self.bulk_done(export, elapsed_ms, result),
            ArchiveInput::DestinationPicked(folder) => {
                self.options.destination = Some(folder);
                ArchiveOutcome::changed()
            }
            ArchiveInput::DestinationFailed(error) => {
                warn!(error = %error, "the destination folder picker failed");
                ArchiveOutcome {
                    toasts: vec![(
                        ToastKind::Danger,
                        format!("Could not open the folder picker: {error}"),
                    )],
                    ..ArchiveOutcome::changed()
                }
            }
        }
    }

    fn search_pressed(&mut self, paths: &DataPaths) -> ArchiveOutcome {
        if self.fetch_in_flight() {
            debug!(
                generation = self.generation,
                "search ignored: a page is still being read"
            );
            return ArchiveOutcome::default();
        }
        let Ok(filters) = self.form.parsed() else {
            debug!("search ignored: the form has issues");
            return ArchiveOutcome::default();
        };
        self.generation += 1;
        self.epoch += 1;
        self.pages = 0;
        self.visible_end = 0;
        self.inspector.reset();
        info!(
            generation = self.generation,
            time_window = filters.time.is_some(),
            text_mode = text_mode(&filters),
            rules = rule_count(&filters),
            "archive search"
        );
        let query = SearchQuery {
            filters,
            limit: NonZeroUsize::new(ARCHIVE_PAGE),
            after: None,
        };
        self.last_query = Some(query.clone());
        self.results = Results::Loading {
            generation: self.generation,
        };
        ArchiveOutcome {
            effects: vec![self.page_request(query, paths)],
            ..ArchiveOutcome::changed()
        }
    }

    fn page_request(&self, query: SearchQuery, paths: &DataPaths) -> ArchiveEffect {
        ArchiveEffect::Search(PageRequest {
            generation: self.generation,
            query,
            paths: paths.clone(),
        })
    }

    /// The last search's query, continuing after `cursor`.
    fn next_page(&self, cursor: SearchCursor) -> Option<SearchQuery> {
        let searched = self.last_query.as_ref()?;
        Some(SearchQuery {
            after: Some(cursor),
            ..searched.clone()
        })
    }

    fn page_loaded(
        &mut self,
        generation: u64,
        elapsed_ms: u128,
        result: Result<SearchPage, String>,
        paths: &DataPaths,
    ) -> ArchiveOutcome {
        if generation != self.generation {
            debug!(generation, "a page of an earlier search was dropped");
            return ArchiveOutcome::default();
        }
        self.pages += 1;
        let page_number = self.pages;
        match (std::mem::take(&mut self.results), result) {
            (Results::Loading { .. }, Ok(page)) => {
                let mut rows = ArchiveRows::default();
                let more = self.land(&mut rows, page, elapsed_ms);
                self.results = Results::Shown {
                    generation,
                    rows,
                    more,
                };
            }
            (Results::Loading { .. }, Err(error)) => {
                warn!(generation, page = page_number, error = %error, "the archive search failed");
                self.results = Results::Unavailable { generation, error };
            }
            (
                Results::Shown {
                    mut rows,
                    more: More::Loading(_),
                    ..
                },
                Ok(page),
            ) => {
                let more = self.land(&mut rows, page, elapsed_ms);
                self.results = Results::Shown {
                    generation,
                    rows,
                    more,
                };
            }
            (
                Results::Shown {
                    rows,
                    more: More::Loading(cursor),
                    ..
                },
                Err(error),
            ) => {
                warn!(generation, page = page_number, error = %error, "loading more results failed");
                self.results = Results::Shown {
                    generation,
                    rows,
                    more: More::Failed { cursor, error },
                };
            }
            (
                other @ (Results::NotSearched
                | Results::Unavailable { .. }
                | Results::Shown {
                    more: More::Ready(_) | More::Failed { .. } | More::End | More::Capped,
                    ..
                }),
                _,
            ) => {
                self.pages -= 1;
                debug!(generation, "a page nothing asked for was dropped");
                self.results = other;
                return ArchiveOutcome::default();
            }
        }
        self.epoch += 1;
        let mut out = self.fetch_if_near_end(paths);
        out.changed = true;
        out
    }

    /// Appends `page` to `rows` and says what comes next, logging the page.
    fn land(&self, rows: &mut ArchiveRows, page: SearchPage, elapsed_ms: u128) -> More {
        let SearchPage {
            items,
            next,
            scanned,
        } = page;
        let items_len = items.len();
        let appended = rows.append(items);
        let more = more_after(rows, appended, next);
        debug!(
            generation = self.generation,
            page = self.pages,
            items = items_len,
            skipped = appended.skipped,
            scanned,
            elapsed_ms,
            more = more.as_str(),
            "archive page loaded"
        );
        match more {
            More::Capped => info!(
                generation = self.generation,
                rows = rows.len(),
                body_mib = rows.body_bytes() / (1024 * 1024),
                "the archive results are full; later matches are not loaded"
            ),
            More::Ready(_) | More::Loading(_) | More::Failed { .. } | More::End => {}
        }
        more
    }

    /// Asks for the next page when more exist, none is being read, and the visible rows are
    /// near the loaded end.
    fn fetch_if_near_end(&mut self, paths: &DataPaths) -> ArchiveOutcome {
        let cursor = match &self.results {
            Results::Shown {
                rows,
                more: More::Ready(cursor),
                ..
            } if self.visible_end + LOAD_AHEAD >= rows.len() => cursor.clone(),
            Results::NotSearched
            | Results::Loading { .. }
            | Results::Unavailable { .. }
            | Results::Shown { .. } => return ArchiveOutcome::default(),
        };
        self.load_after(cursor, paths)
    }

    fn retry_page(&mut self, paths: &DataPaths) -> ArchiveOutcome {
        let cursor = match &self.results {
            Results::Shown {
                more: More::Failed { cursor, .. },
                ..
            } => cursor.clone(),
            Results::NotSearched
            | Results::Loading { .. }
            | Results::Unavailable { .. }
            | Results::Shown { .. } => return ArchiveOutcome::default(),
        };
        self.load_after(cursor, paths)
    }

    /// Moves `Shown` to reading the page after `cursor`.
    fn load_after(&mut self, cursor: SearchCursor, paths: &DataPaths) -> ArchiveOutcome {
        let Some(query) = self.next_page(cursor.clone()) else {
            debug!("no page loaded: the search that showed these rows is gone");
            return ArchiveOutcome::default();
        };
        match &mut self.results {
            Results::Shown { more, .. } => *more = More::Loading(cursor),
            Results::NotSearched | Results::Loading { .. } | Results::Unavailable { .. } => {
                debug!("no page loaded: no results are shown");
                return ArchiveOutcome::default();
            }
        }
        ArchiveOutcome {
            effects: vec![self.page_request(query, paths)],
            ..ArchiveOutcome::changed()
        }
    }

    fn select(&mut self, key_str: &str) -> ArchiveOutcome {
        let row = match &self.results {
            Results::Shown { rows, .. } => rows.by_key(key_str),
            Results::NotSearched | Results::Loading { .. } | Results::Unavailable { .. } => None,
        };
        let Some(row) = row else {
            debug!(key = %key_str, "a pressed result is no longer shown");
            return ArchiveOutcome::default();
        };
        self.inspector.select(Selected::of(row));
        ArchiveOutcome::changed()
    }

    fn step(&mut self, step: Step) -> ArchiveOutcome {
        let position = position_of(self, self.inspector.selected.as_ref());
        let key = step_target(step, position, self.count())
            .and_then(|target| self.row(target))
            .map(|row| row.key_str.clone());
        match key {
            Some(key_str) => self.select(&key_str),
            None => ArchiveOutcome::default(),
        }
    }

    fn inspector_input(&mut self, input: InspectorInput, paths: &DataPaths) -> ArchiveOutcome {
        let cx = InspectorContext {
            section: Section::Export,
            owner: Some(self.folder_epoch),
            paths,
            dropped_answer: DROPPED_ANSWER,
        };
        let out = self.inspector.apply(input, cx);
        ArchiveOutcome {
            effects: out
                .export
                .into_iter()
                .map(ArchiveEffect::ExportOne)
                .collect(),
            changed: out.changed,
            toasts: out.toasts,
            run_failed: false,
        }
    }
}

impl RowSource for ArchiveState {
    fn count(&self) -> usize {
        match &self.results {
            Results::Shown { rows, .. } => rows.len(),
            Results::NotSearched | Results::Loading { .. } | Results::Unavailable { .. } => 0,
        }
    }

    fn row(&self, position: usize) -> Option<&RowSummary> {
        match &self.results {
            Results::Shown { rows, .. } => rows.rows().get(position),
            Results::NotSearched | Results::Loading { .. } | Results::Unavailable { .. } => None,
        }
    }

    fn epoch(&self) -> u64 {
        self.epoch
    }
}

/// How the log names `filters`' text condition (never the text itself).
pub(crate) fn text_mode(filters: &CaptureFilters) -> &'static str {
    match &filters.text {
        None => "none",
        Some(TextFilter::Contains(_)) => "contains",
        Some(TextFilter::Regex(_)) => "regex",
    }
}

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
