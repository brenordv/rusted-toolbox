//! Exporting every match of the last search: the run's state, its options, and its progress.

use super::{ArchiveEffect, ArchiveOutcome, ArchiveState, text_mode};
use crate::read::rules::rule_count;
use crate::state::ToastKind;
use common_utils::string_utils::format_thousands;
use shared_eventhub_persistence::config::ExportSettings;
use shared_eventhub_persistence::export::{ExportFormat, ExportOptions, ExportOutcome};
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::search::SearchQuery;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// At most one progress update per this long.
pub const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// Where the bulk export is.
#[derive(Debug, Default)]
pub enum BulkExport {
    #[default]
    Idle,
    /// Run `export` writes files; `cancel` is the flag the run checks between messages.
    Running {
        export: u64,
        cancel: Arc<AtomicBool>,
        cancel_requested: bool,
        counts: ExportOutcome,
    },
    /// Run `export` ended with its counts, or with the error that stopped it.
    Done {
        export: u64,
        result: Result<ExportOutcome, String>,
    },
}

impl BulkExport {
    pub fn is_running(&self) -> bool {
        match self {
            Self::Running { .. } => true,
            Self::Idle | Self::Done { .. } => false,
        }
    }
}

/// Why a run was asked to stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelReason {
    Button,
    WindowClosed,
}

impl CancelReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::WindowClosed => "window_closed",
        }
    }
}

/// How a bulk export writes its files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BulkOptions {
    pub format: ExportFormat,
    pub metadata: bool,
    /// One file per month instead of one per message.
    pub condense: bool,
    pub local_time: bool,
    pub skip_exported: bool,
    /// The export folder inside the data folder, unless `destination` is set.
    pub export_folder: String,
    /// A folder the user picked instead.
    pub destination: Option<PathBuf>,
}

impl Default for BulkOptions {
    fn default() -> Self {
        Self::from_settings(&ExportSettings::default())
    }
}

impl BulkOptions {
    pub fn from_settings(settings: &ExportSettings) -> Self {
        Self {
            format: settings.format,
            metadata: settings.include_metadata,
            condense: settings.condense_output,
            local_time: settings.use_local_time,
            skip_exported: settings.skip_already_exported,
            export_folder: settings.export_folder.clone(),
            destination: None,
        }
    }

    /// The folder the files go to for the data folder `paths`.
    pub fn destination(&self, paths: &DataPaths) -> PathBuf {
        self.destination
            .clone()
            .unwrap_or_else(|| paths.export_dir(&self.export_folder))
    }

    pub fn export_options(&self, paths: &DataPaths) -> ExportOptions {
        ExportOptions {
            format: self.format,
            include_metadata: self.metadata,
            condense_output: self.condense,
            use_local_time: self.local_time,
            export_folder: self.destination(paths),
            skip_already_exported: self.skip_exported,
        }
    }
}

/// Export every match of `query` with `options`; progress and the end go back tagged `export`.
#[derive(Debug)]
pub struct BulkRequest {
    pub export: u64,
    pub query: SearchQuery,
    pub options: ExportOptions,
    pub cancel: Arc<AtomicBool>,
    pub paths: DataPaths,
}

/// Whether a progress update is due at `now`, the last one having gone out at `last`.
pub fn progress_due(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|last| now.duration_since(last) >= PROGRESS_EVERY)
}

/// Whether a run's end is a failure the user must see: it stopped with an error, or some
/// files failed, whether or not it was cancelled.
pub fn run_failed(result: &Result<ExportOutcome, String>) -> bool {
    match result {
        Ok(outcome) => outcome.failed > 0,
        Err(_) => true,
    }
}

/// The toast for a run where `failed` files could not be written, the first with `error`.
fn failed_files_text(failed: u64, error: &str) -> String {
    match failed {
        1 => format!("1 file could not be written: {error}"),
        _ => format!(
            "{} files could not be written. The first: {error}",
            format_thousands(failed)
        ),
    }
}

/// The counts on one line.
pub fn counts_line(outcome: &ExportOutcome) -> String {
    format!(
        "Exported {} · Skipped {} · Failed {}",
        format_thousands(outcome.exported),
        format_thousands(outcome.skipped),
        format_thousands(outcome.failed)
    )
}

impl ArchiveState {
    pub(super) fn bulk_pressed(&mut self, paths: &DataPaths) -> ArchiveOutcome {
        match &self.bulk {
            BulkExport::Running { export, .. } => {
                debug!(export, "bulk export ignored: one is already running");
                return ArchiveOutcome::default();
            }
            BulkExport::Idle | BulkExport::Done { .. } => {}
        }
        let Some(searched) = &self.last_query else {
            debug!("bulk export ignored: nothing was searched");
            return ArchiveOutcome::default();
        };
        let query = SearchQuery {
            after: None,
            ..searched.clone()
        };
        self.next_export += 1;
        let export = self.next_export;
        let cancel = Arc::new(AtomicBool::new(false));
        let options = self.options.export_options(paths);
        info!(
            export,
            skip_already_exported = options.skip_already_exported,
            condense_output = options.condense_output,
            include_metadata = options.include_metadata,
            use_local_time = options.use_local_time,
            destination_overridden = self.options.destination.is_some(),
            time_window = query.filters.time.is_some(),
            text_mode = text_mode(&query.filters),
            rules = rule_count(&query.filters),
            "bulk export started"
        );
        self.bulk = BulkExport::Running {
            export,
            cancel: Arc::clone(&cancel),
            cancel_requested: false,
            counts: ExportOutcome::default(),
        };
        ArchiveOutcome {
            effects: vec![ArchiveEffect::BulkExport(BulkRequest {
                export,
                query,
                options,
                cancel,
                paths: paths.clone(),
            })],
            ..ArchiveOutcome::changed()
        }
    }

    pub(super) fn bulk_progress(&mut self, export: u64, latest: ExportOutcome) -> ArchiveOutcome {
        match &mut self.bulk {
            BulkExport::Running {
                export: running,
                counts,
                ..
            } if *running == export => {
                *counts = latest;
                ArchiveOutcome::changed()
            }
            BulkExport::Running { .. } | BulkExport::Idle | BulkExport::Done { .. } => {
                debug!(
                    export,
                    "a progress update for another bulk export was dropped"
                );
                ArchiveOutcome::default()
            }
        }
    }

    /// Asks the running export to stop; true when this changed anything.
    pub(crate) fn cancel_bulk(&mut self, reason: CancelReason) -> bool {
        match &mut self.bulk {
            BulkExport::Running {
                export,
                cancel,
                cancel_requested: requested @ false,
                ..
            } => {
                cancel.store(true, Ordering::Release);
                *requested = true;
                info!(
                    export = *export,
                    reason = reason.as_str(),
                    "bulk export cancel requested"
                );
                true
            }
            BulkExport::Running { .. } | BulkExport::Idle | BulkExport::Done { .. } => false,
        }
    }

    pub(super) fn bulk_done(
        &mut self,
        export: u64,
        elapsed_ms: u128,
        result: Result<ExportOutcome, String>,
    ) -> ArchiveOutcome {
        let last = match &self.bulk {
            BulkExport::Running {
                export: running,
                counts,
                ..
            } if *running == export => counts.clone(),
            BulkExport::Running { .. } | BulkExport::Idle | BulkExport::Done { .. } => {
                debug!(export, "the end of another bulk export was dropped");
                return ArchiveOutcome::default();
            }
        };
        let failed = run_failed(&result);
        let toast = match &result {
            Ok(outcome) => {
                info!(
                    export,
                    exported = outcome.exported,
                    skipped = outcome.skipped,
                    failed = outcome.failed,
                    cancelled = outcome.cancelled,
                    elapsed_ms,
                    "bulk export finished"
                );
                outcome
                    .first_error
                    .as_ref()
                    .filter(|_| failed)
                    .map(|error| failed_files_text(outcome.failed, error))
            }
            Err(error) => {
                warn!(
                    export,
                    elapsed_ms,
                    exported = last.exported,
                    skipped = last.skipped,
                    failed = last.failed,
                    error = %error,
                    "bulk export stopped"
                );
                Some(format!("The export stopped: {error}"))
            }
        };
        self.bulk = BulkExport::Done { export, result };
        ArchiveOutcome {
            toasts: toast
                .map(|text| (ToastKind::Danger, text))
                .into_iter()
                .collect(),
            run_failed: failed,
            ..ArchiveOutcome::changed()
        }
    }

    pub(super) fn dismiss_bulk(&mut self) -> ArchiveOutcome {
        match &self.bulk {
            BulkExport::Done { .. } => {
                self.bulk = BulkExport::Idle;
                ArchiveOutcome::changed()
            }
            BulkExport::Idle | BulkExport::Running { .. } => ArchiveOutcome::default(),
        }
    }
}

#[cfg(test)]
mod tests;
