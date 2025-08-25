use anyhow::{Context, Result};
use reqwest::Client;
use std::collections::HashSet;
use std::path::PathBuf;
use tracing::info;

const AI_IGNORE_KNOWN_FILES: &[&str] = &[
    ".aiignore",
    ".cursorignore",
    ".cursorindexingignore",
    ".windsurfignore",
    ".codeiumignore",
    ".windsurfrules",
    ".claudeignore", //Proposed on July 2025, but not implemented yet.
    ".aiexclude",    // Gemni/Bard
];

const AI_IGNORE_TEMPLATES: &[&str] = &[
    "https://raw.githubusercontent.com/brenordv/gitignore-files/refs/heads/master/get-out-of-my-land.ai"
];

pub async fn run_aiignore_maintainer(folder: PathBuf) -> Result<()> {
    let mut aiignore_data: HashSet<String> = HashSet::new();

    info!("Checking for existing AI ignore files...");
    for file_name in AI_IGNORE_KNOWN_FILES {
        let file_path = folder.join(file_name);
        if !file_path.exists() {
            continue;
        }

        info!("Found existing file: {}", file_name);
        let content = std::fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read {}", file_name))?;

        content.lines().for_each(|line| {
            aiignore_data.insert(line.to_string());
        });
    }

    info!("Downloading AI ignore templates...");
    let client = Client::new();
    for url in AI_IGNORE_TEMPLATES {
        get_aiignore_template_data(url, &client, &mut aiignore_data).await?;
    }

    if aiignore_data.is_empty() {
        info!("No AI ignore data available...");
        return Ok(());
    }

    let clean_data = sanitize_aiignore_data(&aiignore_data);

    info!("Writing {} lines to AI ignore files...", clean_data.len());

    for file_name in AI_IGNORE_KNOWN_FILES {
        let file_path = folder.join(file_name);
        let action = if file_path.exists() {
            "updating"
        } else {
            "Creating"
        };

        info!("{} file: {}", action, file_name);
        std::fs::write(&file_path, clean_data.join("\n"))
            .with_context(|| format!("Failed to write {}", file_name))?;
    }

    info!("All done!");
    Ok(())
}

fn sanitize_aiignore_data(aiignore_data: &HashSet<String>) -> Vec<String> {
    let mut data = HashSet::new();

    for line in aiignore_data {
        let trimmed = line.trim();
        if !trimmed.starts_with('#') && !trimmed.is_empty() {
            data.insert(trimmed.to_string());
        }
    }

    let mut sorted_data: Vec<String> = data.into_iter().collect();
    sorted_data.sort();
    sorted_data
}

async fn get_aiignore_template_data(
    url: &str,
    client: &Client,
    aiignore_data: &mut HashSet<String>,
) -> Result<()> {
    let response = client
        .get(url)
        .send()
        .await
        .context("Failed to send HTTP request")?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Error fetching AI ignore data from {}. Status: {}",
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
        if aiignore_data.insert(line) {
            new_lines += 1;
        }
    });

    if new_lines == 0 {
        info!("No new lines of AI ignore data found in {}", url);
    } else {
        info!(
            "Successfully fetched {} lines of AI ignore data from {}",
            new_lines, url
        );
    }

    Ok(())
}
