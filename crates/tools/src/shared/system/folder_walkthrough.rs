use std::path::PathBuf;
use walkdir::WalkDir;

pub fn list_all_files_recursively(path: &PathBuf) -> impl Iterator<Item = PathBuf> {
    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
}
