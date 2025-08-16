use crate::export_progress_tracker::ExportProgressTracker;
use crate::message_exporters::export_message_csv::export_message_csv;
use crate::message_exporters::export_message_json::export_message_json;
use crate::message_exporters::export_message_txt::export_message_txt;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use shared::eventhub::eventhub_models::{EventHubConfig, InboundMessage};
use shared::eventhub::utils::extract_eventhub_endpoint_from_connection_string::extract_eventhub_endpoint_from_connection_string;
use shared::system::resolve_path_with_base::resolve_path_with_base;
use shared::utils::message_matches_filter::message_matches_filter;
use sled::Db;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::fs;

pub struct EventHubExporter {
    config: EventHubConfig,
    source_db: Db,
    export_db: Db,
    progress: Arc<ExportProgressTracker>,
    shutdown: Arc<AtomicBool>,
}

impl EventHubExporter {
    /// Creates a new EventHubExporter instance with initialized databases and resources.
    ///
    /// # Arguments
    /// - `config`: EventHub configuration including connection string and paths
    /// - `shutdown`: Optional atomic boolean for shutdown signaling
    ///
    /// # Returns
    /// - `Ok(Self)`: Successfully initialized exporter
    /// - `Err`: Configuration, database, or permission validation failed
    ///
    /// # Prerequisites
    /// - Source database must exist (created by eh-read with same connection string)
    /// - Write permissions required for export directory
    /// - Valid EventHub endpoint extractable from connection string
    pub async fn new(config: EventHubConfig, shutdown: Option<Arc<AtomicBool>>) -> Result<Self> {
        // TODO #1: Related.
        let endpoint = extract_eventhub_endpoint_from_connection_string(&config.connection_string)?;

        // Setup source database path (same as eh-read)
        let db_base_dir = resolve_path_with_base(
            &config.inbound_config.base_data_folder,
            &config.inbound_config.database_path,
        );

        let source_db_path = db_base_dir.join(format!("{}.db", endpoint));

        // Verify source database exists
        if !source_db_path.exists() {
            return Err(anyhow!(
                "Source database not found at: {:?}. Make sure eh-read has been run first with the same connection string and paths.\n\
                Expected database file: {}.db\n\
                In directory: {:?}",
                source_db_path, endpoint, db_base_dir
            ));
        }

        // Setup export database path
        let export_db_path =
            resolve_path_with_base(&config.export_config.base_data_folder, "export-tracking-db");

        // Ensure export database directory exists
        if let Some(parent) = export_db_path.parent() {
            fs::create_dir_all(parent)
                .await
                .context("Failed to create export database directory")?;
        }

        // Open databases
        let source_db = sled::open(&source_db_path).context(format!(
            "Failed to open source database at [{:?}]",
            source_db_path
        ))?;

        let export_db = sled::open(&export_db_path).context(format!(
            "Failed to open export database at [{:?}]",
            export_db_path
        ))?;

        // Test write permissions
        Self::test_write_permissions(&config).await?;

        //TODO: Check if the u64 will work here (in practical terms).
        let progress = Arc::new(ExportProgressTracker::new(
            config.export_config.feedback_interval as f64,
        ));
        let shutdown = shutdown.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));

        Ok(Self {
            config,
            source_db,
            export_db,
            progress,
            shutdown,
        })
    }

    /// Tests write permissions for the export directory by creating and deleting a test file.
    ///
    /// # Arguments
    /// - `config`: EventHub configuration containing export directory settings
    ///
    /// # Returns
    /// - `Ok(())`: Write permissions verified successfully
    /// - `Err`: Directory creation, file write, or delete operation failed
    async fn test_write_permissions(config: &EventHubConfig) -> Result<()> {
        let export_path = resolve_path_with_base(
            &config.export_config.base_data_folder,
            &config.export_config.export_folder,
        );

        // Create directories if they don't exist
        fs::create_dir_all(&export_path).await.context(format!(
            "Failed to create export directory [{:?}]",
            export_path
        ))?;

        // Test write permissions
        let test_file = export_path.join("__test_write_permissions__.tmp");
        fs::write(&test_file, "test").await.context(format!(
            "Cannot write to export directory [{:?}]",
            export_path
        ))?;

        fs::remove_file(&test_file).await.context(format!(
            "Cannot delete test file in export directory [{:?}]",
            export_path
        ))?;

        println!("✅  Write permissions verified for: {:?}", export_path);
        Ok(())
    }

    /// Starts the export process, iterating through source database messages and exporting filtered ones.
    ///
    /// # Behavior
    /// - Processes messages with "msg:" key prefix from source database
    /// - Applies export filters and checkpoint validation
    /// - Exports eligible messages in configured format
    /// - Tracks progress with real-time feedback
    /// - Supports graceful shutdown via atomic flag
    ///
    /// # Returns
    /// - `Ok(())`: Export completed successfully
    /// - `Err`: Database, deserialization, or export operation failed
    pub async fn start_export(&self) -> Result<()> {
        println!("🚀 Starting export process...");

        // Iterate through all messages in a source database
        for item in self.source_db.iter() {
            if self.shutdown.load(Ordering::Relaxed) {
                println!("\n🛑 Shutdown signal received, stopping export...");
                break;
            }

            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);

            // Only process message keys.
            // In the eventhub_reader_app::process_received_event (private method) we create the
            // key for the messages with the prefix "msg:".
            if !key_str.starts_with("msg:") {
                continue;
            }

            // Deserialize the message
            let message: InboundMessage =
                serde_json::from_slice(&value).context("Failed to deserialize message")?;

            // Check if we should export this message
            if self.should_export_message(&message, &key_str).await? {
                self.export_message(&message, &key_str).await?;
                self.progress.increment_exported();
            } else {
                self.progress.increment_duplicated();
            }

            // Show progress
            if self.progress.should_show_progress() {
                self.progress.print_progress();
            }
        }

        // Final progress update
        self.progress.print_progress();
        println!();
        println!("✅  Export completed successfully!");

        Ok(())
    }

    /// Determines whether a message should be exported based on filters and checkpoint status.
    ///
    /// # Arguments
    /// - `message`: InboundMessage to evaluate for export eligibility
    /// - `key`: Unique message key for database lookup
    ///
    /// # Returns
    /// - `Ok(true)`: Message should be exported
    /// - `Ok(false)`: Message should be skipped (already exported, filtered out, or duplicate)
    /// - `Err`: Database or filter evaluation error
    ///
    /// # Logic
    /// - Checks export checkpoint unless `ignore_checkpoint` is enabled
    /// - Validates file existence for non-condensed output
    /// - Applies dump filters if configured
    async fn should_export_message(&self, message: &InboundMessage, key: &str) -> Result<bool> {
        // Check if already exported (unless ignoring checkpoint)
        if !self.config.export_config.ignore_checkpoint {
            if let Ok(Some(_)) = self.export_db.get(key.as_bytes()) {
                // For condense_output=false, check if a file still exists
                if !self.config.export_config.condense_output {
                    let file_path = self.get_message_file_path(message);
                    if !file_path.exists() {
                        // File was deleted, re-export
                        return Ok(true);
                    }
                }
                return Ok(false);
            }
        }

        // Check dump filter
        if let Some(filters) = &self.config.export_config.dump_filter {
            if !filters.is_empty() && !message_matches_filter(&message.msg_data, filters) {
                self.progress.increment_skipped();
                return Ok(false);
            }
            return Ok(true);
        }

        Ok(true)
    }

    /// Generates the file path for storing a message based on configuration and timestamp.
    ///
    /// # Arguments
    /// - `message`: InboundMessage containing timestamp information
    ///
    /// # Returns
    /// - `PathBuf`: Generated file path with year/month/day structure
    ///
    /// # Path Structure
    /// - Base: `base_data_folder/export_folder`
    /// - Subdirectories: `YYYY-MM/[DD]/` (day subdirectory only if not condensed)
    /// - Filename: Generated based on timestamp and export format
    fn get_message_file_path(&self, message: &InboundMessage) -> PathBuf {
        let export_base = resolve_path_with_base(
            &self.config.export_config.base_data_folder,
            &self.config.export_config.export_folder,
        );

        let (year_month, day) =
            message.get_file_safe_timestamp_yyyy_mm_dd(self.config.export_config.use_local_time);

        let mut path = export_base.join(&year_month);

        if !self.config.export_config.condense_output {
            path = path.join(&day);
        }

        let filename = self.generate_filename(message);
        path.join(&filename)
    }

    /// Generates a filename for the exported message based on configuration.
    ///
    /// # Arguments
    /// - `message`: InboundMessage containing timestamp and event ID
    ///
    /// # Returns
    /// - `String`: Generated filename with appropriate extension
    ///
    /// # Filename Patterns
    /// - Condensed: `messages-YYYY-MM.{ext}`
    /// - Individual: `{timestamp}-{event_id}.{ext}`
    fn generate_filename(&self, message: &InboundMessage) -> String {
        let extension = match self.config.export_config.export_format.as_str() {
            "csv" => "csv",
            "json" => "json",
            _ => "txt",
        };

        if self.config.export_config.condense_output {
            // For condensed output, use a simple filename based on year-month
            let (year_month, _) = message
                .get_file_safe_timestamp_yyyy_mm_dd(self.config.export_config.use_local_time);
            format!("messages-{}.{}", year_month, extension)
        } else {
            // For individual files, use timestamp-based naming
            let base_name =
                message.get_file_safe_timestamp_full(self.config.export_config.use_local_time);
            format!("{}-{}.{}", base_name, message.event_id, extension)
        }
    }

    /// Exports a message to file in the configured format and marks it as exported.
    ///
    /// # Arguments
    /// - `message`: InboundMessage to export
    /// - `key`: Message key for tracking export status
    ///
    /// # Returns
    /// - `Ok(())`: Message exported and marked successfully
    /// - `Err`: Export operation or database update failed
    ///
    /// # Formats
    /// - TXT: Plain text with optional metadata
    /// - CSV: Comma-separated values with headers
    /// - JSON: JSON objects with metadata
    async fn export_message(&self, message: &InboundMessage, key: &str) -> Result<()> {
        let file_path = self.get_message_file_path(message);
        let include_metadata = self.config.export_config.include_metadata;
        let condense_output = self.config.export_config.condense_output;
        let entity_path = &self.config.entity_path;

        match self.config.export_config.export_format.as_str() {
            "txt" => {
                export_message_txt(message, &file_path, include_metadata, condense_output).await?
            }
            "csv" => {
                export_message_csv(
                    message,
                    &file_path,
                    include_metadata,
                    condense_output,
                    entity_path,
                )
                .await?
            }
            "json" => {
                export_message_json(
                    message,
                    &file_path,
                    include_metadata,
                    condense_output,
                    entity_path,
                )
                .await?
            }
            _ => {
                return Err(anyhow!(
                    "Unsupported export format: {}",
                    self.config.export_config.export_format
                ))
            }
        }

        // Mark as exported
        let export_timestamps = vec![Utc::now()];
        let export_data = serde_json::to_vec(&export_timestamps)?;
        self.export_db.insert(key.as_bytes(), export_data)?;

        Ok(())
    }

    /// Initiates graceful shutdown by setting shutdown flag and flushing databases.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        println!("\n🔄 Shutdown signal received, initiating cleanup...");
        // Flush databases
        let _ = self.export_db.flush();
    }
}
