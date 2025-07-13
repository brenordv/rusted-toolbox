use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

pub trait EnsureDirectoryExists {
    fn ensure_directory_exists(&self) -> Result<()>;
}

impl EnsureDirectoryExists for PathBuf {
    fn ensure_directory_exists(&self) -> Result<()> {
        if self.exists() {
            return Ok(());
        }

        Ok(fs::create_dir_all(&self).context("Failed to create directory")?)
    }
}
