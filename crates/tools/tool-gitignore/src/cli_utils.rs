use crate::models::GitIgnoreConfig;
use anyhow::Result;
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_utils::constants::CONFIG_UL_ITEM_LEVEL_2;
use common_utils::file_system::get_current_dir;
use std::path::{Path, PathBuf};

/// Creates/updates a gitignore file based on current content.
///
/// Automatically creates or updates `.gitignore` files based on detected file types in your project.
#[derive(Parser, Debug)]
#[command(about, long_about, version)]
pub struct CliArgs {
    /// Target folder to analyze
    #[arg(num_args = 1, required = false)]
    pub target_folder: Option<PathBuf>,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

fn print_runtime_info(target_folder: &Path) {
    println!(
        "{} Target folder: {}\n",
        CONFIG_UL_ITEM_LEVEL_2,
        target_folder.display()
    );
}

pub fn initialize() -> Result<GitIgnoreConfig> {
    let args = CliArgs::parse();

    let target_folder = args.target_folder.unwrap_or_else(get_current_dir);

    if !target_folder.is_dir() {
        anyhow::bail!("Target folder does not exist, we don't have permission to read it or it is not a directory.");
    }

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| {
            print_runtime_info(&target_folder);
        }),
    );

    Ok(GitIgnoreConfig { target_folder })
}
