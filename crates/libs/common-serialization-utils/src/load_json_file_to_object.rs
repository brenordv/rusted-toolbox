use anyhow::{anyhow, Result};
use serde::de::DeserializeOwned;
use std::path::Path;
use tokio::fs;

/// Loads a JSON file and deserializes it into the specified type.
///
/// # Arguments
/// * `path` - The path to the JSON file to load
///
/// # Returns
/// * `Result<T>` - The deserialized object of type T on success, or an error
///
/// # Errors
/// * Returns an error if the file cannot be read
/// * Returns an error if the JSON cannot be parsed
/// * Returns an error if the JSON structure doesn't match the expected type T
pub async fn load_json_file_to_object<T>(path: &Path) -> Result<T>
where
    T: DeserializeOwned,
{
    let content = fs::read_to_string(path)
        .await
        .map_err(|e| anyhow!("Failed to read file {:?}: {}", path, e))?;

    // Remove a leading UTF-8 BOM if present
    let content = content.trim_start_matches('\u{FEFF}');

    let object: T = serde_json::from_str(content)
        .map_err(|e| anyhow!("Failed to parse JSON from file {:?}: {}", path, e))?;

    Ok(object)
}