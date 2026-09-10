use crate::config::{Config, AI_ARTIFACTS_TEMPLATE_URL};
use crate::models::GitIgnoreConfig;
use anyhow::{Context, Result};
use reqwest::Client;
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use walkdir::WalkDir;

/// Order-preserving line accumulator: keeps the first occurrence of each line
/// in insertion order. gitignore semantics are last-match-wins, so preserving
/// line order matters; a plain set would let a sort or hash iteration hoist
/// `!` re-include lines above the patterns they negate.
struct UniqueLines {
    lines: Vec<String>,
    seen: HashSet<String>,
}

impl UniqueLines {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            seen: HashSet::new(),
        }
    }

    /// Appends the line if it has not been seen before. Returns true when the
    /// line is new.
    fn insert(&mut self, line: String) -> bool {
        if self.seen.contains(&line) {
            return false;
        }
        self.seen.insert(line.clone());
        self.lines.push(line);
        true
    }

    fn len(&self) -> usize {
        self.lines.len()
    }

    fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

pub async fn run_gitignore_maintainer(app_config: GitIgnoreConfig) -> Result<()> {
    let target_gitignore = app_config.target_folder.join(".gitignore");
    let config = Config::new();
    let mut gitignore_data = UniqueLines::new();

    info!("Figuring out which .gitignore files to download...");
    let pending_urls = collect_pending_urls(
        &app_config.target_folder,
        &target_gitignore,
        &config,
        app_config.include_ai,
    );

    if pending_urls.is_empty() {
        info!("No new .gitignore data to download. Guess I won't touch the .gitignore...");
        return Ok(());
    }

    info!("Fetching new .gitignore data...");
    let client = Client::new();
    for url in &pending_urls {
        get_gitignore_data(url, &client, &mut gitignore_data).await?;
    }

    if gitignore_data.is_empty() {
        info!("No new .gitignore data available...");
        return Ok(());
    }

    info!(
        "Fetched {} lines of data for the .gitignore file...",
        gitignore_data.len()
    );
    dump_gitignore_data(&target_gitignore, &gitignore_data.lines)?;

    info!("All done!");
    Ok(())
}

/// Walks the target folder and collects the template URLs to download: one
/// per detected footprint key, plus the AI artifacts template when
/// `include_ai` is set. The set is sorted by URL so the download (and
/// therefore merge) order is stable run to run.
fn collect_pending_urls(
    folder: &Path,
    target_gitignore: &Path,
    config: &Config,
    include_ai: bool,
) -> BTreeSet<String> {
    let mut keys_found: HashSet<String> = HashSet::new();
    let mut pending_urls: BTreeSet<String> = BTreeSet::new();

    for path in list_files(folder, target_gitignore) {
        let Some(path_str) = path.to_str() else {
            warn!(path = %path.display(), "Skipping non-UTF-8 path");
            continue;
        };

        let new_keys =
            config.update_map_keys_for_file(path_str, &mut keys_found, &mut pending_urls);

        if new_keys.is_empty() {
            continue;
        }
        let new_keys_string: String = new_keys.join(", ");
        info!(
            "New .gitignore data queued for download: {}",
            new_keys_string
        );
    }

    if include_ai {
        info!(url = %AI_ARTIFACTS_TEMPLATE_URL, "AI artifacts template queued by --ai");
        pending_urls.insert(AI_ARTIFACTS_TEMPLATE_URL.to_string());
    }

    pending_urls
}

/// Writes the merged .gitignore: the existing file's lines first, in their
/// original order, then the fetched lines in fetch order. The combined list
/// goes through [`sanitize_gitignore_data`] before hitting disk.
fn dump_gitignore_data(target_gitignore: &Path, fetched_lines: &[String]) -> Result<()> {
    let mut merged: Vec<String> = Vec::new();

    if target_gitignore.exists() {
        info!("The .gitignore already exists. Merging with the new data...");
        let existing_content = std::fs::read_to_string(target_gitignore)
            .context("Failed to read existing .gitignore")?;

        merged.extend(existing_content.lines().map(|s| s.to_string()));
    }

    merged.extend(fetched_lines.iter().cloned());

    let clean_data = sanitize_gitignore_data(&merged);

    info!("Writing {} lines to .gitignore...", clean_data.len());

    std::fs::write(target_gitignore, clean_data.join("\n"))
        .context("Failed to write .gitignore file")?;

    Ok(())
}

/// Drops comment lines and blanks, trims whitespace, and deduplicates with the
/// first occurrence winning. Line order is preserved: sorting a .gitignore can
/// invert its meaning by moving `!` re-include lines ahead of the patterns
/// they negate.
fn sanitize_gitignore_data(gitignore_data: &[String]) -> Vec<String> {
    let mut clean = UniqueLines::new();

    for line in gitignore_data {
        let trimmed = line.trim();
        if !trimmed.starts_with('#') && !trimmed.is_empty() {
            clean.insert(trimmed.to_string());
        }
    }

    clean.lines
}

async fn get_gitignore_data(
    url: &str,
    client: &Client,
    github_data: &mut UniqueLines,
) -> Result<()> {
    let response = client
        .get(url)
        .send()
        .await
        .context("Failed to send HTTP request")?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Error fetching gitignore data from {}. Status: {}",
            url,
            response.status()
        );
    }

    let text = response
        .text()
        .await
        .context("Failed to read response text")?;

    let mut new_lines: usize = 0;
    text.lines().map(|s| s.to_string()).for_each(|line| {
        if github_data.insert(line) {
            new_lines += 1;
        }
    });

    if new_lines == 0 {
        info!("No new lines of gitignore data found in {}", url);
    } else {
        info!(
            "Successfully fetched {} lines of gitignore data from {}",
            new_lines, url
        );
    }

    Ok(())
}

fn list_files(base: &Path, target_gitignore: &Path) -> Vec<PathBuf> {
    WalkDir::new(base)
        .into_iter()
        .filter_map(|entry| entry.ok()) // skip errors gracefully
        .filter(|entry| !should_ignore(entry.path(), target_gitignore))
        .map(|entry| entry.into_path())
        .collect()
}

fn should_ignore(path: &Path, target_gitignore: &Path) -> bool {
    is_git_related(path) // Skipping git folders
        || is_in_ignorable_file(path) // Skipping files that are not relevant
        || is_gitignore_file(path) // Skipping other gitignore files
        || path == target_gitignore // Skipping the target gitignore file
}

fn is_git_related(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()) == Some(".git")
        || path
            .ancestors()
            .any(|p| p.file_name().and_then(|n| n.to_str()) == Some(".git"))
}

fn is_in_ignorable_file(path: &Path) -> bool {
    const IGNORABLE_FOLDERS: &[&str] = &[
        ".git",
        "node_modules",
        "venv",
        ".venv",
        "__pycache__",
        "env",
        "build",
        "dist",
        "bin",
        ".idea",
        ".vs",
        ".vscode",
        // AI agent state dirs: their contents must not feed detection (a
        // .claude/hooks/foo.py is not a Python project), but the dirs
        // themselves still match their footprint keys.
        ".claude",
        ".cursor",
        ".windsurf",
        ".gemini",
        ".continue",
        ".cline",
        ".codex",
        ".codeium",
    ];

    if path.is_dir() {
        // We can keep the folders, since they may hint for things to ignore (like .vscode, .idea, etc.)
        return false;
    }

    path.ancestors().any(|p| {
        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
            IGNORABLE_FOLDERS.contains(&name.to_lowercase().as_str())
        } else {
            false
        }
    })
}

fn is_gitignore_file(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()) == Some(".gitignore")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_lines_keeps_insertion_order_and_first_occurrence() {
        let mut lines = UniqueLines::new();

        assert!(lines.insert("zebra/".to_string()));
        assert!(lines.insert("alpha/".to_string()));
        assert!(!lines.insert("zebra/".to_string()));

        assert_eq!(lines.len(), 2);
        assert_eq!(lines.lines, vec!["zebra/", "alpha/"]);
    }

    #[test]
    fn sanitize_gitignore_data_drops_comments_and_blanks_without_sorting() {
        let input = vec![
            "# a comment".to_string(),
            "   ".to_string(),
            "zebra/".to_string(),
            "  *.log  ".to_string(),
            String::new(),
            "alpha/".to_string(),
            "*.log".to_string(),
        ];

        let result = sanitize_gitignore_data(&input);

        assert_eq!(result, vec!["zebra/", "*.log", "alpha/"]);
    }

    #[test]
    fn is_gitignore_file_detects_gitignore_name() {
        assert!(is_gitignore_file(Path::new(".gitignore")));
        assert!(!is_gitignore_file(Path::new("main.rs")));
    }

    #[test]
    fn is_git_related_detects_dot_git_anywhere() {
        assert!(is_git_related(Path::new(".git")));
        assert!(is_git_related(Path::new(".git/config")));
        assert!(!is_git_related(Path::new("src/main.rs")));
    }

    #[test]
    fn is_in_ignorable_file_detects_ignorable_ancestor() {
        assert!(is_in_ignorable_file(Path::new("node_modules/pkg/index.js")));
        assert!(!is_in_ignorable_file(Path::new("src/main.rs")));
    }

    #[test]
    fn should_ignore_flags_gitignore_but_not_source_files() {
        let target = Path::new("/project/.gitignore");

        assert!(should_ignore(Path::new("/project/.gitignore"), target));
        assert!(!should_ignore(Path::new("/project/src/app.rs"), target));
    }

    #[test]
    fn collect_pending_urls_seeds_artifacts_template_only_when_include_ai_is_set() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".gitignore");
        let config = Config::new();

        let without_ai = collect_pending_urls(dir.path(), &target, &config, false);
        let with_ai = collect_pending_urls(dir.path(), &target, &config, true);

        assert!(without_ai.is_empty());
        assert_eq!(
            with_ai.into_iter().collect::<Vec<_>>(),
            vec![AI_ARTIFACTS_TEMPLATE_URL.to_string()]
        );
    }

    #[test]
    fn collect_pending_urls_detects_agent_dir_but_ignores_its_contents() {
        let dir = tempfile::tempdir().unwrap();
        let claude_dir = dir.path().join(".claude");
        std::fs::create_dir(&claude_dir).unwrap();
        std::fs::write(claude_dir.join("hook.py"), "print()").unwrap();
        let target = dir.path().join(".gitignore");
        let config = Config::new();

        let urls = collect_pending_urls(dir.path(), &target, &config, false);

        // The .claude dir itself queues the artifacts template; the .py file
        // inside it must not queue the Python template.
        assert_eq!(
            urls.into_iter().collect::<Vec<_>>(),
            vec![AI_ARTIFACTS_TEMPLATE_URL.to_string()]
        );
    }

    #[test]
    fn dump_gitignore_data_writes_clean_file_in_fetch_order() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".gitignore");
        let data = vec![
            "*.pyc".to_string(),
            "# comment".to_string(),
            "*.log".to_string(),
        ];

        dump_gitignore_data(&target, &data).unwrap();

        let content = std::fs::read_to_string(&target).unwrap();
        assert_eq!(content, "*.pyc\n*.log");
    }

    #[test]
    fn dump_gitignore_data_keeps_existing_lines_first_in_their_original_order() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".gitignore");
        std::fs::write(&target, "zebra.tmp\nalpha.tmp\n*.log\n# old comment").unwrap();
        let data = vec![
            "*.pyc".to_string(),
            "*.log".to_string(),
            "target/".to_string(),
        ];

        dump_gitignore_data(&target, &data).unwrap();

        let content = std::fs::read_to_string(&target).unwrap();
        // Existing lines come first and keep their relative order (no sorting),
        // fetched lines follow in fetch order, and the duplicated "*.log"
        // collapses to its first occurrence.
        assert_eq!(content, "zebra.tmp\nalpha.tmp\n*.log\n*.pyc\ntarget/");
    }

    #[test]
    fn dump_gitignore_data_keeps_negations_after_the_patterns_they_negate() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".gitignore");
        std::fs::write(&target, "*.log\n!keep.log").unwrap();
        let data = vec!["*.pyc".to_string()];

        dump_gitignore_data(&target, &data).unwrap();

        // gitignore is last-match-wins: "!keep.log" only re-includes the file
        // while it stays after "*.log".
        let content = std::fs::read_to_string(&target).unwrap();
        assert_eq!(content, "*.log\n!keep.log\n*.pyc");
    }
}
