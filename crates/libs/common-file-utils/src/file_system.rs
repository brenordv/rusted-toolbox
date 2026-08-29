use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Recursively lists every file under `path`.
///
/// Directory entries that cannot be read (permission errors, races) are silently
/// skipped rather than surfaced, and a `path` that is a plain file yields just
/// that file. Both follow from the underlying `walkdir` traversal with failed
/// entries filtered out.
pub fn list_all_files_recursively(path: &Path) -> impl Iterator<Item = PathBuf> {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn lists_every_file_across_nested_directories() {
        let root = tempdir().unwrap();
        let nested = root.path().join("a").join("b");
        fs::create_dir_all(&nested).unwrap();

        let root_file = root.path().join("root.txt");
        let a_file = root.path().join("a").join("a.txt");
        let b_file = nested.join("b.txt");
        fs::write(&root_file, "").unwrap();
        fs::write(&a_file, "").unwrap();
        fs::write(&b_file, "").unwrap();

        let found: HashSet<PathBuf> = list_all_files_recursively(root.path()).collect();

        let expected: HashSet<PathBuf> = [root_file, a_file, b_file].into_iter().collect();
        assert_eq!(found, expected);
    }

    #[test]
    fn empty_directory_yields_no_files() {
        let root = tempdir().unwrap();

        let found: Vec<PathBuf> = list_all_files_recursively(root.path()).collect();

        assert!(found.is_empty());
    }

    #[test]
    fn a_file_path_yields_just_that_file() {
        let root = tempdir().unwrap();
        let file = root.path().join("only.txt");
        fs::write(&file, "").unwrap();

        let found: Vec<PathBuf> = list_all_files_recursively(&file).collect();

        assert_eq!(found, vec![file]);
    }
}
