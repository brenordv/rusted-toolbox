use crate::models::LookupConfig;
use anyhow::{anyhow, Context, Result};
use shared::system::folder_walkthrough::list_all_files_recursively;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::error;

pub fn lookup_files(config: &LookupConfig) -> Result<()> {
    let start = Instant::now();

    let base_path = PathBuf::from(&config.path);
    if !base_path.exists() {
        let err_msg = format!("Path does not exist: {}", base_path.display());
        error!("{}", err_msg);
        return Err(anyhow!(err_msg));
    }

    let normalized_extensions = normalize_extensions(&config.file_extensions);
    let needle = config.text.to_ascii_lowercase();

    let files_iter = list_files(&base_path, config.current_only)?;
    let mut files_read: u64 = 0;
    let mut total_lines: u64 = 0;
    let mut matches_found: u64 = 0;

    for file_path in files_iter {
        if !path_matches_allowed(&file_path, &normalized_extensions) {
            continue;
        }

        let file = match File::open(&file_path) {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to open file '{}': {}", file_path.display(), e);
                continue;
            },
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

// Accept patterns like "txt", ".txt", "*.txt", "Md", ".env", "*.env", "env"
fn normalize_extensions(exts: &[String]) -> Vec<String> {
    // Normalize to lowercase; keep either a bare extension (e.g., "txt")
    // or a leading-dot basename pattern (e.g., ".env") if user intends filename match.
    exts.iter()
        .filter_map(|raw| {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return None;
            }
            let lower = trimmed.to_ascii_lowercase();

            // Remove a single leading '*' for patterns like "*.txt" or "*.env"
            let no_glob = lower.strip_prefix('*').unwrap_or(&lower);

            // If it starts with a dot and contains no further dots (like ".env"),
            // preserve the leading dot to indicate basename matching for dotfiles.
            if no_glob.starts_with('.') && !no_glob[1..].contains('.') {
                return Some(no_glob.to_string());
            }

            // Otherwise, strip a single leading dot to treat as pure extension ("txt", "md")
            let no_dot = no_glob.strip_prefix('.').unwrap_or(no_glob);
            if no_dot.is_empty() {
                None
            } else {
                Some(no_dot.to_string())
            }
        })
        .collect()
}

// Decides if a path is allowed by the normalized patterns.
// - If a pattern list is empty -> allow all files.
// - If a file has extension -> match against extension entries (e.g., "rs", "md").
// - Always also try basename match for entries like ".env" or "env".
fn path_matches_allowed(path: &Path, normalized: &[String]) -> bool {
    if normalized.is_empty() {
        return true;
    }

    // Check basename match for patterns like ".env" or "env"
    if matches_basename(path, normalized) {
        return true;
    }

    // Fallback to extension matching (e.g., "rs", "md", "txt")
    if let Some(ext_os) = path.extension() {
        let ext = ext_os.to_string_lossy().to_ascii_lowercase();
        return normalized.iter().any(|e| e == &ext);
    }

    false
}

// Allows matching by exact file name for dotfiles or bare names:
// - Pattern ".env" matches basename ".env"
// - Pattern "env" matches basename "env"
// Note: We only consider entries in `normalized` that either start with '.' or have no '.' at all.
fn matches_basename(path: &Path, normalized: &[String]) -> bool {
    let Some(name) = path.file_name().map(|s| s.to_string_lossy().to_string()) else {
        return false;
    };
    let name_lc = name.to_ascii_lowercase();

    normalized.iter().any(|p| {
        // Treat entries like ".env" or "env" as basename candidates
        if p.starts_with('.') || !p.contains('.') {
            p == &name_lc
        } else {
            false
        }
    })
}

fn list_files(path: &Path, current_only: bool) -> Result<Box<dyn Iterator<Item = PathBuf>>> {
    // If the input is a file, return that single file directly.
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

fn get_search_dir(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    }
}