//! Exporting archived messages to txt, csv, or json files, and the disk-write
//! modes that dump messages while they are captured.

mod formats;
mod render;
mod writing_sink;

pub use render::format_full_message;
pub use writing_sink::{DiskWriteConfig, DiskWriteMode, WritingSink};

use crate::search::SearchQuery;
use crate::store::MessageStore;
use crate::store::schema::EXPORT_LOG;
use anyhow::Context;
use chrono::Utc;
use common_utils::string_utils::sanitize_string_for_filename;
use formats::{Layout, write_message};
use redb::ReadableTable;
use render::{file_timestamp, month_and_day};
use serde::{Deserialize, Serialize};
use shared_eventhub_core::models::{CapturedMessage, MessageKey};
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tracing::{info, warn};

/// Messages read per archive page during an export.
const EXPORT_PAGE: usize = 512;
/// Per-file export failures logged one by one before the rest are only
/// counted.
const LOGGED_FAILURES: u64 = 20;

/// The file format of an export.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    #[default]
    Txt,
    Csv,
    Json,
}

impl ExportFormat {
    /// The file extension for this format, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Txt => "txt",
            Self::Csv => "csv",
            Self::Json => "json",
        }
    }
}

/// How and where archived messages are exported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportOptions {
    pub format: ExportFormat,
    /// Write the details (txt) or the metadata columns (csv, json) along with
    /// the body.
    pub include_metadata: bool,
    /// One file per month instead of one file per message.
    pub condense_output: bool,
    /// Date folders and file names in local time instead of UTC.
    pub use_local_time: bool,
    pub export_folder: PathBuf,
    /// Skip messages the export log says were already exported (an individual
    /// file that no longer exists is exported again).
    pub skip_already_exported: bool,
}

/// The counts of an export run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportOutcome {
    pub exported: u64,
    pub skipped: u64,
    pub failed: u64,
    /// The first failure, with its file path, when any file failed.
    pub first_error: Option<String>,
    /// The run stopped early because its cancel flag was set; matching
    /// messages after that point were not exported.
    pub cancelled: bool,
}

impl ExportOutcome {
    /// Counts a failed file: the first 20 are logged with their path, then
    /// one note says the rest are only counted.
    pub(crate) fn record_failure(&mut self, path: &Path, error: &anyhow::Error) {
        self.failed += 1;
        let rendered = format!("{error:#}");
        if self.failed <= LOGGED_FAILURES {
            warn!(path = %path.display(), error = %rendered, "Failed to write a message file");
        } else if self.failed == LOGGED_FAILURES + 1 {
            warn!("More message files failed; they are counted but no longer logged");
        }
        self.first_error
            .get_or_insert_with(|| format!("{}: {rendered}", path.display()));
    }
}

impl MessageStore {
    /// Exports every message matching `query`. A file that fails is counted
    /// and the export continues; `progress` is called after each message;
    /// `cancel` is checked between messages, so every file written stays
    /// complete for its format.
    ///
    /// # Errors
    /// Fails when the archive cannot be read or the export log cannot be
    /// updated.
    pub fn export(
        &self,
        query: &SearchQuery,
        options: &ExportOptions,
        mut progress: impl FnMut(&ExportOutcome),
        cancel: &AtomicBool,
    ) -> anyhow::Result<ExportOutcome> {
        let entity_path = self.entity_path()?;
        let mut outcome = ExportOutcome::default();
        let mut page_query = SearchQuery {
            filters: query.filters.clone(),
            limit: NonZeroUsize::new(EXPORT_PAGE),
            after: query.after.clone(),
        };
        let started = Instant::now();
        info!(
            folder = %options.export_folder.display(),
            format = ?options.format,
            "Starting an export"
        );

        loop {
            let page = self.search(&page_query)?;
            let logged = if options.skip_already_exported {
                self.exported_keys(&page.items)?
            } else {
                HashSet::new()
            };
            let mut exported = Vec::new();
            let mut cancelled = false;
            for message in &page.items {
                if cancel.load(Ordering::Relaxed) {
                    cancelled = true;
                    break;
                }
                let path = export_path(options, message);
                if logged.contains(&message.key) && (options.condense_output || path.exists()) {
                    outcome.skipped += 1;
                } else {
                    match write_message(&path, message, layout(options, &entity_path)) {
                        Ok(()) => {
                            outcome.exported += 1;
                            exported.push(message.key.clone());
                        }
                        Err(error) => outcome.record_failure(&path, &error),
                    }
                }
                progress(&outcome);
            }
            self.record_exports(&exported)?;

            outcome.cancelled = cancelled;
            match page.next {
                Some(next) if !cancelled => page_query.after = Some(next),
                _ => break,
            }
        }

        info!(
            exported = outcome.exported,
            skipped = outcome.skipped,
            failed = outcome.failed,
            elapsed_ms = started.elapsed().as_millis(),
            "Finished an export"
        );
        Ok(outcome)
    }

    /// Writes the message stored under `key` to `destination` in the options'
    /// format, replacing an existing file. The export log is not touched: this
    /// is a copy, not an export run.
    ///
    /// # Errors
    /// Fails when no message is stored under `key`, or the file cannot be
    /// written.
    pub fn export_one(
        &self,
        key: &MessageKey,
        options: &ExportOptions,
        destination: &Path,
    ) -> anyhow::Result<()> {
        let message = self.get(key)?.with_context(|| {
            format!(
                "No message is stored at partition {}, sequence {}",
                key.partition_id, key.sequence_number
            )
        })?;
        let entity_path = self.entity_path()?;
        let single = Layout {
            condense: false,
            ..layout(options, &entity_path)
        };

        write_message(destination, &message, single)
    }

    /// The event hub name written in the `entity_path` column; empty for an
    /// archive that was never bound (it holds no messages either).
    fn entity_path(&self) -> anyhow::Result<String> {
        Ok(self
            .binding()?
            .map(|hub| hub.event_hub_name)
            .unwrap_or_default())
    }

    fn exported_keys(&self, messages: &[CapturedMessage]) -> anyhow::Result<HashSet<MessageKey>> {
        let read = self.begin_read()?;
        let export_log = read
            .open_table(EXPORT_LOG)
            .context("Failed to open the archive's export log")?;
        let mut keys = HashSet::new();
        for message in messages {
            let key = (
                message.key.partition_id.as_str(),
                message.key.sequence_number,
            );
            if export_log
                .get(key)
                .context("Failed to read the export log")?
                .is_some()
            {
                keys.insert(message.key.clone());
            }
        }
        Ok(keys)
    }

    /// Records `keys` as exported now, in one durable write.
    fn record_exports(&self, keys: &[MessageKey]) -> anyhow::Result<()> {
        if keys.is_empty() {
            return Ok(());
        }
        let now = Utc::now().timestamp_millis();
        let transaction = self
            .database()
            .begin_write()
            .context("Failed to start an export-log write")?;
        {
            let mut export_log = transaction
                .open_table(EXPORT_LOG)
                .context("Failed to open the archive's export log")?;
            for key in keys {
                export_log
                    .insert((key.partition_id.as_str(), key.sequence_number), now)
                    .context("Failed to update the export log")?;
            }
        }
        transaction.commit().with_context(|| {
            format!(
                "Failed to commit the export log of '{}'",
                self.path().display()
            )
        })
    }
}

fn layout<'a>(options: &ExportOptions, entity_path: &'a str) -> Layout<'a> {
    Layout {
        format: options.format,
        include_metadata: options.include_metadata,
        condense: options.condense_output,
        entity_path,
    }
}

/// `<folder>/<YYYY-MM>/messages-<YYYY-MM>.<ext>` when condensed, otherwise
/// `<folder>/<YYYY-MM>/<DD>/<timestamp>-<partition>-<sequence>.<ext>`, dated
/// by when the message was captured.
fn export_path(options: &ExportOptions, message: &CapturedMessage) -> PathBuf {
    let extension = options.format.extension();
    let (month, day) = month_and_day(message.processed_at, options.use_local_time);
    let month_folder = options.export_folder.join(&month);

    if options.condense_output {
        month_folder.join(format!("messages-{month}.{extension}"))
    } else {
        month_folder.join(day).join(format!(
            "{}-{}-{}.{extension}",
            file_timestamp(message.processed_at, options.use_local_time),
            sanitize_string_for_filename(&message.key.partition_id),
            message.key.sequence_number
        ))
    }
}

#[cfg(test)]
mod tests;
