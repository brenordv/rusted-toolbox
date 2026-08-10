use crate::cat_app::cat_file;
use crate::cli_utils::get_cli_arguments;
use crate::models::CatOptions;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::tool_exit_helpers::{exit_error, exit_success};
use std::io;
use std::io::Write;
use tracing::error;

mod cat_app;
mod cli_utils;
mod models;

fn main() {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Error);

    let args = get_cli_arguments();

    let options = CatOptions::from_args(&args);

    // Handle -u flag by setting stdout to unbuffered
    if args.u_flag {
        io::stdout().flush().unwrap_or(());
    }

    if args.files.is_empty() {
        // No files specified, read from stdin
        if let Err(e) = cat_file(None, &options) {
            error!("Failed to run CAT from stdin: {}", e);
            exit_error();
        }
    } else {
        // Process each file
        for filename in &args.files {
            if let Err(e) = cat_file(Some(filename), &options) {
                error!("Failed to run CAT from file [{}]: {}", filename, e);
                exit_error();
            }
        }
    }

    exit_success();
}
