use anyhow::{Context, Result};
use shared::eventhub::eventhub_models::InboundMessage;
use shared::system::ensure_directory_exists::EnsureDirectoryExists;
use std::path::PathBuf;
use tokio::fs;

/// Exports an InboundMessage to a text file with optional metadata and condensed output.
///
/// # Arguments
/// - `message`: InboundMessage to export
/// - `file_path`: Target file path for text output
/// - `include_metadata`: Whether to include full message formatting with metadata
/// - `condense_output`: Whether to append to existing file with separators or overwrite
///
/// # Returns
/// - `Ok(())`: Message exported successfully
/// - `Err`: Directory creation or file I/O failed
///
/// # Behavior
/// - Metadata enabled: Uses full message formatting with metadata
/// - Metadata disabled: Exports only message data content
/// - Condensed: Appends with separators (\n or \n\n)
/// - Individual: Overwrites existing file
pub async fn export_message_txt(
    message: &InboundMessage,
    file_path: &PathBuf,
    include_metadata: bool,
    condense_output: bool,
) -> Result<()> {
    file_path.ensure_directory_exists()?;

    let content = if include_metadata {
        message.format_full_message_to_string()
    } else {
        message.msg_data.clone()
    };

    if condense_output && file_path.exists() {
        // For condensed output, append to an existing file with a separator
        let separator = if include_metadata { "\n\n" } else { "\n" };

        let mut file_content = fs::read_to_string(&file_path)
            .await
            .context("Failed to read existing file")?;

        file_content.push_str(separator);

        file_content.push_str(&content);

        fs::write(&file_path, file_content)
            .await
            .context("Failed to write to existing file")?;
    } else {
        // For individual files or new condensed files, write directly
        fs::write(&file_path, content)
            .await
            .context("Failed to write to file")?;
    }

    Ok(())
}
