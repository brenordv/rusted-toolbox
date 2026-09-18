use crate::models::GitIgnoreConfig;
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::header_format::format_config_item;
use common_cli::tool_exit_helpers::exit_error;
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

    /// Also queue the AI agent artifacts template (Agents.gitignore), even
    /// when no agent footprint is detected in the target folder
    #[arg(long)]
    pub ai: bool,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

fn print_runtime_info(target_folder: &Path, include_ai: bool) {
    println!(
        "{}\n{}\n",
        format_config_item("Target folder", target_folder.display()),
        format_config_item("Include AI artifacts", include_ai)
    );
}

/// Parses command-line arguments into the tool configuration.
///
/// Validates the target folder, then boots logging and the optional app
/// header. A missing or unreadable target folder is reported on stderr and
/// terminates the program: logging is not installed yet at that point.
pub fn initialize() -> GitIgnoreConfig {
    let args = CliArgs::parse();

    let target_folder = args.target_folder.unwrap_or_else(get_current_dir);
    let include_ai = args.ai;

    if !target_folder.is_dir() {
        // Logging is not installed yet at this point, so report on stderr directly.
        eprintln!(
            "Error: Target folder does not exist, we don't have permission to read it or it is not a directory."
        );
        exit_error();
    }

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| {
            print_runtime_info(&target_folder, include_ai);
        }),
    );

    GitIgnoreConfig {
        target_folder,
        include_ai,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_flag_parses_true_when_present() {
        let args = CliArgs::try_parse_from(["gitignore", "--ai"]).unwrap();

        assert!(args.ai);
    }

    #[test]
    fn ai_flag_defaults_to_false_when_absent() {
        let args = CliArgs::try_parse_from(["gitignore"]).unwrap();

        assert!(!args.ai);
    }
}
