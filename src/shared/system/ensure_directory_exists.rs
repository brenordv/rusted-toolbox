use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;

#[async_trait]
pub trait EnsureDirectoryExists {
    async fn ensure_directory_exists(&self) -> Result<()>;
}

#[async_trait]
impl EnsureDirectoryExists for PathBuf {
    async fn ensure_directory_exists(&self) -> Result<()> {
        if let Some(parent) = self.parent() {
            Ok(fs::create_dir_all(parent)
                .await
                .context("Failed to create directory")?)
        } else {
            Ok(())
        }
    }
}
