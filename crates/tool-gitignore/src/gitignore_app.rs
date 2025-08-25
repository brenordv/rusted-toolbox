use crate::config::Config;
use anyhow::{Context, Result};
use reqwest::Client;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::info;
use walkdir::WalkDir;

pub async fn run_gitignore_maintainer(folder: PathBuf) -> Result<()> {
    let target_gitignore = folder.join(".gitignore");
    let config = Config::new();
    let mut keys_found: HashSet<String> = HashSet::new();
    let mut pending_urls: HashSet<String> = HashSet::new();
    let mut gitignore_data: HashSet<String> = HashSet::new();

    info!("Figuring out which .gitignore files to download...");
    list_files(folder, &target_gitignore)
        .iter()
        .for_each(|path| {
            let path_str = path.to_str().unwrap().to_string();
            let new_keys =
                config.update_map_keys_for_file(&path_str, &mut keys_found, &mut pending_urls);

            if new_keys.is_empty() {
                return;
            }
            let new_keys_string: String = new_keys.join(", ");
            info!(
                "New .gitignore data queued for download: {}",
                new_keys_string
            );
        });

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
    dump_gitignore_data(&target_gitignore, &mut gitignore_data)?;

    info!("All done!");
    Ok(())
}

fn dump_gitignore_data(
    target_gitignore: &PathBuf,
    github_data: &mut HashSet<String>,
) -> Result<()> {
    if target_gitignore.exists() {
        info!("The .gitignore already exists. Merging with the new data...");
        let existing_content = std::fs::read_to_string(target_gitignore)
            .context("Failed to read existing .gitignore")?;

        let existing_lines: Vec<String> = existing_content.lines().map(|s| s.to_string()).collect();
        github_data.extend(existing_lines);
    }

    let clean_data = sanitize_gitignore_data(github_data);

    info!("Writing {} lines to .gitignore...", clean_data.len());

    std::fs::write(target_gitignore, clean_data.join("\n"))
        .context("Failed to write .gitignore file")?;

    Ok(())
}

fn sanitize_gitignore_data(gitignore_data: &HashSet<String>) -> Vec<String> {
    let mut data = HashSet::new();

    for line in gitignore_data {
        let trimmed = line.trim();
        if !trimmed.starts_with('#') && !trimmed.is_empty() {
            data.insert(trimmed.to_string());
        }
    }

    let mut sorted_data: Vec<String> = data.into_iter().collect();
    sorted_data.sort();
    sorted_data
}

async fn get_gitignore_data(
    url: &str,
    client: &Client,
    github_data: &mut HashSet<String>,
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

fn list_files(base: PathBuf, target_gitignore: &PathBuf) -> Vec<PathBuf> {
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
