use anyhow::{Context, Result};
use common_file_utils::file_system::list_all_files_recursively;
use std::path::{Path, PathBuf};

// Accept patterns like "txt", ".txt", "*.txt", "Md", ".env", "*.env", "env"
pub fn normalize_extensions(exts: &[String]) -> Vec<String> {
    exts.iter()
        .filter_map(|raw| {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return None;
            }
            let lower = trimmed.to_ascii_lowercase();
            let no_glob = lower.strip_prefix('*').unwrap_or(&lower);
            if no_glob.starts_with('.') && !no_glob[1..].contains('.') {
                return Some(no_glob.to_string());
            }
            let no_dot = no_glob.strip_prefix('.').unwrap_or(no_glob);
            if no_dot.is_empty() {
                None
            } else {
                Some(no_dot.to_string())
            }
        })
        .collect()
}

pub fn path_matches_allowed(path: &Path, normalized: &[String]) -> bool {
    if normalized.is_empty() {
        return true;
    }
    if matches_basename(path, normalized) {
        return true;
    }
    if let Some(ext_os) = path.extension() {
        let ext = ext_os.to_string_lossy().to_ascii_lowercase();
        return normalized.iter().any(|e| e == &ext);
    }
    false
}

pub fn matches_basename(path: &Path, normalized: &[String]) -> bool {
    let Some(name) = path.file_name().map(|s| s.to_string_lossy().to_string()) else {
        return false;
    };
    let name_lc = name.to_ascii_lowercase();

    normalized.iter().any(|p| {
        if p.starts_with('.') || !p.contains('.') {
            p == &name_lc
        } else {
            false
        }
    })
}

pub fn list_files(path: &Path, current_only: bool) -> Result<Box<dyn Iterator<Item = PathBuf>>> {
    if path.is_file() {
        return Ok(Box::new(std::iter::once(path.to_path_buf())));
    }

    let dir = get_search_dir(path);
    if current_only {
        let iter = std::fs::read_dir(&dir)
            .with_context(|| format!("Failed to read directory '{}'", dir.display()))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file());
        Ok(Box::new(iter))
    } else {
        Ok(Box::new(list_all_files_recursively(&dir)))
    }
}

pub fn get_search_dir(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn normalize_extensions_handles_various_forms() {
        assert_eq!(normalize_extensions(&["txt".to_string()]), ["txt"]);
        assert_eq!(normalize_extensions(&[".txt".to_string()]), [".txt"]);
        assert_eq!(normalize_extensions(&["*.txt".to_string()]), [".txt"]);
        assert_eq!(normalize_extensions(&["Md".to_string()]), ["md"]);
        assert_eq!(normalize_extensions(&["env".to_string()]), ["env"]);
        assert_eq!(normalize_extensions(&[".env".to_string()]), [".env"]);
    }

    #[test]
    fn normalize_extensions_drops_empty_and_bare_glob() {
        let result = normalize_extensions(&["".to_string(), "   ".to_string(), "*".to_string()]);
        assert!(result.is_empty());
    }

    #[test]
    fn path_matches_allowed_true_when_no_filter() {
        assert!(path_matches_allowed(Path::new("anything.bin"), &[]));
    }

    #[test]
    fn path_matches_allowed_matches_by_extension() {
        let allowed = normalize_extensions(&["txt".to_string()]);
        assert!(path_matches_allowed(Path::new("notes.txt"), &allowed));
        assert!(!path_matches_allowed(Path::new("notes.rs"), &allowed));
    }

    #[test]
    fn path_matches_allowed_matches_dotfile_by_basename() {
        let allowed = normalize_extensions(&[".env".to_string()]);
        assert!(path_matches_allowed(Path::new(".env"), &allowed));
    }

    #[test]
    fn matches_basename_matches_extensionless_name() {
        assert!(matches_basename(
            Path::new("Makefile"),
            &["makefile".to_string()]
        ));
        assert!(!matches_basename(
            Path::new("readme.md"),
            &["md".to_string()]
        ));
    }

    #[test]
    fn get_search_dir_returns_dir_itself() {
        let dir = tempdir().unwrap();
        assert_eq!(get_search_dir(dir.path()), dir.path().to_path_buf());
    }

    #[test]
    fn get_search_dir_returns_parent_for_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("a.txt");
        fs::write(&file, "x").unwrap();
        assert_eq!(get_search_dir(&file), dir.path().to_path_buf());
    }

    #[test]
    fn list_files_single_file_yields_that_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("a.txt");
        fs::write(&file, "x").unwrap();

        let files: Vec<_> = list_files(&file, false).unwrap().collect();

        assert_eq!(files, vec![file]);
    }

    #[test]
    fn list_files_current_only_skips_subdirectories() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        fs::write(dir.path().join("b.txt"), "x").unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("c.txt"), "x").unwrap();

        let files: Vec<_> = list_files(dir.path(), true).unwrap().collect();

        assert_eq!(files.len(), 2);
    }
}
