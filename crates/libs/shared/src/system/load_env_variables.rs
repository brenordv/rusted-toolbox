use anyhow::{Context, Result};
use dotenv::from_path;
use std::env;

/// Attempts to load a `.env` file from:
/// 1. The executable’s directory
/// 2. The current working directory
///
/// Returns `Ok(())` if a file is loaded, otherwise the last error.
pub fn load_env_variables() -> Result<()> {
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let env_path = exe_dir.join(".env");
            if env_path.exists() {
                from_path(env_path)
                    .context("Failed to load .env file from executable directory")?;
                return Ok(());
            }
        }
    }

    if let Ok(cwd) = env::current_dir() {
        let env_path = cwd.join(".env");
        if env_path.exists() {
            from_path(env_path)
                .context("Failed to load .env file from current working directory")?;
            return Ok(());
        }
    }

    anyhow::bail!("Failed to load .env file")
}
