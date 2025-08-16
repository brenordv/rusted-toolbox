use anyhow::{Context, Result};
use dialoguer::Select;
use std::fs;

pub fn select_file_from_path(path: &str, prompt: &str) -> Result<String> {
    let entries: Vec<String> = fs::read_dir(path)
        .context(format!("Failed to read directory: {}", path))?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().unwrap().is_file())
        .map(|e| e.file_name().into_string().unwrap_or_default())
        .collect();

    let selection = Select::new()
        .with_prompt(prompt)
        .items(&entries)
        .interact()
        .context("Failed to select file")?;

    Ok(entries[selection].to_string())
}
