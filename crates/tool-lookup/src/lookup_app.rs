use crate::models::LookupConfig;
use anyhow::{anyhow, Context, Result};
use shared::system::folder_walkthrough::list_all_files_recursively;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub fn lookup_files(config: &LookupConfig) -> Result<()> {
    let start = Instant::now();

    let base_path = PathBuf::from(&config.path);
    if !base_path.exists() {
        return Err(anyhow!("Path does not exist: {}", base_path.display()));
    }

    let normalized_extensions = normalize_extensions(&config.file_extensions);
    let needle = config.text.to_ascii_lowercase();

    let files_iter = list_files(&base_path, config.current_only)?;
    let mut files_read: u64 = 0;
    let mut total_lines: u64 = 0;
    let mut matches_found: u64 = 0;

    for file_path in files_iter {
        if !path_matches_extensions(&file_path, &normalized_extensions) {
            continue;
        }

        let file = match File::open(&file_path) {
            Ok(f) => f,
            Err(_) => continue, // Skip unreadable files silently
        };
        files_read += 1;

        let reader = BufReader::new(file);
        for (idx, line_res) in reader.lines().enumerate() {
            let line = match line_res {
                Ok(l) => l,
                Err(_) => continue, // Skip problematic lines
            };
            total_lines += 1;

            if line.to_ascii_lowercase().contains(&needle) {
                matches_found += 1;
                if config.line_only {
                    println!("{}", line);
                } else {
                    println!("{}:{}| {}", file_path.display(), idx + 1, line);
                }
            }
        }
    }

    if !config.no_header {
        let elapsed = start.elapsed();
        eprintln!(
            "Searched in {} files, {} lines, {} matches. Took {:?}.",
            files_read, total_lines, matches_found, elapsed
        );
    }

    Ok(())
}

fn normalize_extensions(exts: &[String]) -> Vec<String> {
    // Accept patterns like "txt", ".txt", "*.txt", "Md"
    // Normalize to lowercase without leading dots or wildcards: "txt", "md"
    exts.iter()
        .filter_map(|raw| {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return None;
            }
            let lower = trimmed.to_ascii_lowercase();
            let no_glob = lower.strip_prefix("*").unwrap_or(&lower);
            let no_dot = no_glob.strip_prefix(".").unwrap_or(no_glob);
            if no_dot.is_empty() {
                None
            } else {
                Some(no_dot.to_string())
            }
        })
        .collect()
}

fn path_matches_extensions(path: &Path, normalized_exts: &[String]) -> bool {
    if normalized_exts.is_empty() {
        // If no extensions provided, match all files
        return true;
    }
    let Some(ext_os) = path.extension() else {
        return false;
    };
    let ext = ext_os.to_string_lossy().to_ascii_lowercase();
    normalized_exts.iter().any(|e| e == &ext)
}

fn list_files(path: &Path, current_only: bool) -> Result<Box<dyn Iterator<Item = PathBuf>>> {
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

fn get_search_dir(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    }
}
