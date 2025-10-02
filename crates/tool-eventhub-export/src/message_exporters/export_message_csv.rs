use anyhow::{Context, Result};
use csv::Writer;
use shared::system::ensure_directory_exists::EnsureDirectoryExists;
use std::path::PathBuf;
use shared_eventhub::eventhub_models::InboundMessage;

/// Exports an InboundMessage to a CSV file with optional metadata and condensed output.
///
/// # Arguments
/// - `message`: InboundMessage to export
/// - `file_path`: Target file path for CSV output
/// - `include_metadata`: Whether to include entity path, partition ID, event ID, and timestamp columns
/// - `condense_output`: Whether to append to existing file or overwrite
/// - `entity_path`: Entity path to include in metadata when enabled
///
/// # Returns
/// - `Ok(())`: Message exported successfully
/// - `Err`: Directory creation, file I/O, or CSV writing failed
///
/// # Behavior
/// - Headers written only for new files or non-condensed output
/// - Condensed: Appends rows to existing file
/// - Individual: Overwrites existing file with headers
pub async fn export_message_csv(
    message: &InboundMessage,
    file_path: &PathBuf,
    include_metadata: bool,
    condense_output: bool,
    entity_path: &String,
) -> Result<()> {
    file_path.ensure_directory_exists()?;

    let file_exists = file_path.exists();

    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(file_exists && condense_output)
        .truncate(!file_exists || !condense_output)
        .open(file_path)?;

    let mut writer = Writer::from_writer(file);

    // Only write a header if a file is new, or we're not condensing
    let write_header = !file_exists || !condense_output;

    if include_metadata {
        if write_header {
            writer.write_record([
                "entity_path",
                "partition_id",
                "event_id",
                "timestamp",
                "message_content",
            ])?;
        }

        // Write data
        writer.write_record([
            entity_path,
            &message.partition_id,
            &message.event_id,
            &message
                .processed_at
                .format("%Y-%m-%dT%H:%M:%S%.9fZ")
                .to_string(),
            &message.msg_data,
        ])?;
    } else {
        if write_header {
            writer
                .write_record(["message_content"])
                .context("Failed to write CSV header")?;
        }
        // Write data
        writer
            .write_record([&message.msg_data])
            .context("Failed to write CSV data")?;
    }

    writer.flush().context("Failed to flush CSV writer")?;
    Ok(())
}
