use anyhow::{Context, Result};
use std::path::PathBuf;

pub fn get_tool_path() -> Result<PathBuf> {
    let exe_path = std::env::current_exe()
        .context("Failed to resolve executable path for database default")?;

    let exe_dir = exe_path
        .parent()
        .context("Executable path has no parent directory")?;

    Ok(exe_dir.to_path_buf())
}
