use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::path::Path;
use tokio::fs;

/// Loads a JSON file and deserializes it into the specified type.
///
/// A leading UTF-8 BOM is stripped before parsing.
///
/// # Arguments
/// * `path` - The path to the JSON file to load
///
/// # Returns
/// * `Result<T>` - The deserialized object of type T on success, or an error
///
/// # Errors
/// * Returns an error with context `failed to read file <path>` when the file
///   cannot be read.
/// * Returns an error with context `failed to parse JSON from <path>` when the
///   content is not valid JSON or does not match the expected type T.
pub async fn load_json_file_to_object<T>(path: &Path) -> Result<T>
where
    T: DeserializeOwned,
{
    let content = fs::read_to_string(path)
        .await
        .with_context(|| format!("failed to read file {}", path.display()))?;

    // Remove a leading UTF-8 BOM if present
    let content = content.trim_start_matches('\u{FEFF}');

    let object: T = serde_json::from_str(content)
        .with_context(|| format!("failed to parse JSON from {}", path.display()))?;

    Ok(object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use tempfile::tempdir;

    #[derive(Deserialize, PartialEq, Debug)]
    struct Sample {
        name: String,
        count: u32,
    }

    async fn write_and_load(content: &str) -> Result<Sample> {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.json");
        fs::write(&path, content).await.unwrap();
        load_json_file_to_object::<Sample>(&path).await
    }

    #[tokio::test]
    async fn loads_well_formed_json() {
        let sample = write_and_load(r#"{"name":"widget","count":3}"#)
            .await
            .unwrap();

        assert_eq!(
            sample,
            Sample {
                name: "widget".to_string(),
                count: 3,
            }
        );
    }

    #[tokio::test]
    async fn strips_leading_utf8_bom() {
        let sample = write_and_load("\u{FEFF}{\"name\":\"widget\",\"count\":3}")
            .await
            .unwrap();

        assert_eq!(
            sample,
            Sample {
                name: "widget".to_string(),
                count: 3,
            }
        );
    }

    #[tokio::test]
    async fn malformed_json_reports_parse_error() {
        let error = write_and_load("{ not json }").await.unwrap_err();

        assert!(format!("{error:#}").contains("parse"));
    }

    #[tokio::test]
    async fn missing_file_reports_read_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");

        let error = load_json_file_to_object::<Sample>(&path).await.unwrap_err();

        assert!(format!("{error:#}").contains("read"));
    }
}
