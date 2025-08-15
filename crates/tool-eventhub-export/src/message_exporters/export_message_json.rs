use anyhow::{Context, Result};
use shared::eventhub::eventhub_models::InboundMessage;
use shared::system::ensure_directory_exists::EnsureDirectoryExists;
use std::path::PathBuf;
use tokio::fs;

/// Exports an InboundMessage to a JSON file with optional metadata and condensed output.
///
/// # Arguments
/// - `message`: InboundMessage to export
/// - `file_path`: Target file path for JSON output
/// - `include_metadata`: Whether to include entity path, partition ID, event ID, and timestamp
/// - `condense_output`: Whether to append to existing JSON array or write individual file
/// - `entity_path`: Entity path to include in metadata when enabled
///
/// # Returns
/// - `Ok(())`: Message exported successfully
/// - `Err`: Directory creation, file I/O, or JSON serialization failed
///
/// # Behavior
/// - Condensed: Appends to JSON array, creates array if file doesn't exist
/// - Individual: Writes single JSON object, overwrites existing file
pub async fn export_message_json(
    message: &InboundMessage,
    file_path: &PathBuf,
    include_metadata: bool,
    condense_output: bool,
    entity_path: &String,
) -> Result<()> {
    file_path.ensure_directory_exists()?;

    let message_obj = if include_metadata {
        // The entity_path is required here since we're working with export of a specific entity
        serde_json::json!({
            "entity_path": entity_path,
            "partition_id": message.partition_id,
            "event_id": message.event_id,
            "timestamp": message.processed_at.format("%Y-%m-%dT%H:%M:%S%.9fZ").to_string(),
            "message_content": message.msg_data
        })
    } else {
        serde_json::json!({
            "message_content": message.msg_data
        })
    };

    if condense_output {
        // For condensed output, append to a JSON array
        let mut messages_array = if file_path.exists() {
            // Read existing file and parse as JSON array
            let content = fs::read_to_string(&file_path)
                .await
                .context("Failed to read existing file")?;
            if content.trim().is_empty() {
                serde_json::Value::Array(vec![])
            } else {
                serde_json::from_str(&content).unwrap_or_else(|_| serde_json::Value::Array(vec![]))
            }
        } else {
            serde_json::Value::Array(vec![])
        };

        // Add a new message to the array
        if let serde_json::Value::Array(ref mut arr) = messages_array {
            arr.push(message_obj);
        }

        // Write back the array
        let content = serde_json::to_string_pretty(&messages_array)
            .context("Failed to serialize JSON array")?;

        fs::write(&file_path, content)
            .await
            .context("Failed to write JSON array to file")?;
    } else {
        // For individual files, write a single object
        let content = serde_json::to_string_pretty(&message_obj)
            .context("Failed to serialize JSON object")?;

        fs::write(&file_path, content)
            .await
            .context("Failed to write JSON object to file")?;
    }

    Ok(())
}
