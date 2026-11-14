//! The disk layout of a data folder.

use anyhow::Context;
use common_file_utils::permissions::create_dir_all_owner_only;
use std::path::{Path, PathBuf};
use tracing::info;

const ARCHIVE_FILE_NAME: &str = "archive.redb";
const SECRETS_FILE_NAME: &str = "secrets.redb";
const GITIGNORE_FILE_NAME: &str = ".gitignore";
/// Ignores everything, the ignore file included: the folder holds the message
/// archive and the secrets file, neither of which may ever be committed.
const GITIGNORE_CONTENT: &str = "*\n";

/// A data folder and every path inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataPaths {
    base: PathBuf,
}

impl DataPaths {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    /// `<base>/archive.redb`: messages, time index, checkpoints, export log.
    pub fn archive_db(&self) -> PathBuf {
        self.base.join(ARCHIVE_FILE_NAME)
    }

    /// `<base>/secrets.redb`: the stored connection string and client secrets.
    pub fn secrets_db(&self) -> PathBuf {
        self.base.join(SECRETS_FILE_NAME)
    }

    /// The folder disk-write dumps go to: `received_msg_path` relative to the
    /// data folder, or as given when it is absolute.
    pub fn inbound_dir(&self, received_msg_path: &str) -> PathBuf {
        self.base.join(received_msg_path)
    }

    /// Creates the data folder (owner-only on Unix) when missing and, while
    /// it is empty, writes a `.gitignore` that ignores everything in it. A
    /// folder that already holds files is left as it is.
    ///
    /// # Errors
    /// Fails when the folder cannot be created or listed, or the `.gitignore`
    /// cannot be written.
    pub fn ensure_base(&self) -> anyhow::Result<()> {
        create_dir_all_owner_only(&self.base).with_context(|| {
            format!("Failed to create the data folder '{}'", self.base.display())
        })?;
        let is_empty = std::fs::read_dir(&self.base)
            .with_context(|| format!("Failed to list the data folder '{}'", self.base.display()))?
            .next()
            .is_none();
        if is_empty {
            let gitignore = self.base.join(GITIGNORE_FILE_NAME);
            std::fs::write(&gitignore, GITIGNORE_CONTENT)
                .with_context(|| format!("Failed to write '{}'", gitignore.display()))?;
            info!(path = %self.base.display(), "Created the data folder");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_sit_inside_the_base_folder() {
        let paths = DataPaths::new("data");

        assert_eq!(paths.archive_db(), Path::new("data").join("archive.redb"));
        assert_eq!(paths.secrets_db(), Path::new("data").join("secrets.redb"));
        assert_eq!(
            paths.inbound_dir("inbound"),
            Path::new("data").join("inbound")
        );
    }

    #[test]
    fn an_absolute_inbound_folder_is_kept_as_given() {
        let absolute = tempfile::tempdir().unwrap();
        let paths = DataPaths::new("data");

        let inbound = paths.inbound_dir(absolute.path().to_str().unwrap());

        assert_eq!(inbound, absolute.path());
    }

    #[test]
    fn ensure_base_creates_the_folder_with_a_gitignore() {
        let root = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(root.path().join("nested").join(".eh-data"));

        paths.ensure_base().unwrap();

        let gitignore = std::fs::read_to_string(paths.base().join(".gitignore")).unwrap();
        assert_eq!(gitignore, "*\n");
    }

    #[test]
    fn ensure_base_protects_an_existing_empty_folder() {
        let root = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(root.path());

        paths.ensure_base().unwrap();

        assert!(root.path().join(".gitignore").exists());
    }

    #[test]
    fn ensure_base_leaves_a_folder_that_holds_files_alone() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("notes.txt"), "mine").unwrap();
        let paths = DataPaths::new(root.path());

        paths.ensure_base().unwrap();

        assert!(!root.path().join(".gitignore").exists());
    }

    #[test]
    fn ensure_base_fails_when_the_base_is_a_file() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("not-a-folder");
        std::fs::write(&file, "x").unwrap();

        let error = DataPaths::new(&file).ensure_base().unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Failed to create the data folder"),
            "{error:#}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_created_base_folder_has_no_group_or_other_bits() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let paths = DataPaths::new(root.path().join("data"));

        paths.ensure_base().unwrap();

        let mode = std::fs::metadata(paths.base())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
    }
}
