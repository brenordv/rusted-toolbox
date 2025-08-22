use crate::models::GitIgnoreArgs;
use anyhow::Result;
use clap::{Arg, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;
use shared::system::get_current_working_dir::get_current_working_dir;
use std::path::PathBuf;

pub fn print_runtime_info(args: &GitIgnoreArgs) {
    println!("📋 Gitignore v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);
    println!("Target folder: {}\n", args.target_folder.display());
}

pub fn validate_args(args: &GitIgnoreArgs) -> Result<()> {
    if !args.target_folder.is_dir() {
        anyhow::bail!("Target folder does not exist, we don't have permission to read it or it is not a directory.");
    }

    Ok(())
}

pub fn get_cli_arguments() -> Result<GitIgnoreArgs> {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_DESCRIPTION"),
            "Automatically creates or updates `.gitignore` files based on detected file types in your project.",
        )
        .arg(Arg::new("target-dir")
            .help("Target folder to analyze")
            .num_args(1)
            .required(false)
        )
        .get_matches();

    let target_folder = matches
        .get_one::<String>("target-dir")
        .map(|s| PathBuf::from(s))
        .unwrap_or_else(|| get_current_working_dir());

    Ok(GitIgnoreArgs { target_folder })
}
