use anyhow::{Context, Result};
use std::path::PathBuf;

/// Returns the directory containing the running executable, used by tools to
/// place a default database or data file next to the binary.
///
/// # Errors
/// Returns an error when the executable path cannot be resolved or has no parent
/// directory.
pub fn get_tool_path() -> Result<PathBuf> {
    let exe_path = std::env::current_exe()
        .context("Failed to resolve executable path for database default")?;

    let exe_dir = exe_path
        .parent()
        .context("Executable path has no parent directory")?;

    Ok(exe_dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_tool_path_returns_an_existing_directory() {
        let path = get_tool_path().unwrap();

        assert!(path.is_dir());
    }
}
