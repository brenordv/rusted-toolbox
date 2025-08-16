use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

pub trait EnsureDirectoryExists {
    fn ensure_directory_exists(&self) -> Result<()>;
    fn ensure_parent_exists(&self) -> Result<()>;
}

impl EnsureDirectoryExists for PathBuf {
    fn ensure_directory_exists(&self) -> Result<()> {
        if self.exists() {
            return Ok(());
        }

        fs::create_dir_all(self).context("Failed to create directory")
    }

    fn ensure_parent_exists(&self) -> Result<()> {
        let parent = self.parent().context("Failed to get parent directory")?;

        if parent.exists() {
            return Ok(());
        }

        fs::create_dir_all(parent).context("Failed to create directory")
    }
}
