use anyhow::{Context, Result};
use shared::system::folder_walkthrough::list_all_files_recursively;
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