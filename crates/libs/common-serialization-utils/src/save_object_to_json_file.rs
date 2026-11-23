use anyhow::{Context, Result};
use common_file_utils::atomic_write::write_via_temp;
use serde::Serialize;
use std::fs;
use std::path::Path;

/// Saves `value` to `path` as pretty-printed JSON with a final newline.
///
/// The parent folder is created when it is missing. The file is replaced atomically, through
/// a temporary file beside it: a failed save leaves an existing file as it was.
///
/// # Errors
/// * `failed to serialize JSON for <path>` when `value` cannot be serialized; nothing is
///   written.
/// * `failed to create folder <parent>` when the parent folder cannot be created.
/// * `failed to write file <path>` when the file cannot be written or moved into place.
pub fn save_object_to_json_file<T>(path: &Path, value: &T) -> Result<()>
where
    T: Serialize + ?Sized,
{
    let mut content = serde_json::to_string_pretty(value)
        .with_context(|| format!("failed to serialize JSON for {}", path.display()))?;
    content.push('\n');

    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create folder {}", parent.display()))?;
    }

    write_via_temp(path, |temp_file| {
        fs::write(temp_file, &content).map_err(Into::into)
    })
    .with_context(|| format!("failed to write file {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load_json_file_to_object;
    use serde::{Deserialize, Serializer};
    use tempfile::tempdir;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Sample {
        name: String,
        count: u32,
    }

    fn sample() -> Sample {
        Sample {
            name: "widget".to_string(),
            count: 3,
        }
    }

    struct Unserializable;

    impl Serialize for Unserializable {
        fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("cannot be serialized"))
        }
    }

    #[tokio::test]
    async fn saves_pretty_json_ending_in_a_newline_that_loads_back_equal() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.json");

        save_object_to_json_file(&path, &sample()).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "{\n  \"name\": \"widget\",\n  \"count\": 3\n}\n");
        let loaded: Sample = load_json_file_to_object(&path).await.unwrap();
        assert_eq!(loaded, sample());
    }

    #[test]
    fn creates_a_missing_parent_folder() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested").join("deeper").join("sample.json");

        save_object_to_json_file(&path, &sample()).unwrap();

        assert!(path.is_file());
    }

    #[test]
    fn replaces_an_existing_files_content() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.json");
        fs::write(
            &path,
            "old content, much longer than the new JSON will ever be",
        )
        .unwrap();

        save_object_to_json_file(&path, &sample()).unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\n  \"name\": \"widget\",\n  \"count\": 3\n}\n"
        );
    }

    #[test]
    fn a_value_that_cannot_be_serialized_leaves_the_file_untouched() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.json");
        fs::write(&path, b"original").unwrap();

        let error = save_object_to_json_file(&path, &Unserializable).unwrap_err();

        assert!(format!("{error:#}").contains("failed to serialize JSON"));
        assert_eq!(fs::read(&path).unwrap(), b"original");
    }

    #[test]
    fn a_parent_that_is_a_file_reports_the_folder() {
        let dir = tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        fs::write(&blocker, b"a file").unwrap();
        let path = blocker.join("sample.json");

        let error = save_object_to_json_file(&path, &sample()).unwrap_err();

        assert!(format!("{error:#}").contains("failed to create folder"));
        assert_eq!(fs::read(&blocker).unwrap(), b"a file");
    }

    #[test]
    fn a_destination_that_is_a_folder_reports_the_write() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("occupied");
        fs::create_dir(&path).unwrap();

        let error = save_object_to_json_file(&path, &sample()).unwrap_err();

        assert!(format!("{error:#}").contains("failed to write file"));
        assert!(path.is_dir());
    }
}
