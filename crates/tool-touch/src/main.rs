use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::tool_exit_helpers::{exit_error, exit_success};
use crate::cli_utils::{get_cli_arguments, validate_cli_arguments};
use crate::touch_app::touch_file;
use tracing::error;

mod cli_utils;
mod models;
mod touch_app;

/// Updates file access and modification times, creating files if they don't exist.
///
/// Mimics Unix `touch` command behavior by setting timestamps to current time
/// or user-specified values. Processes multiple files sequentially.
///
/// # Returns
/// - `Ok(())` on successful completion of all file operations
/// - `Err(Box<dyn std::error::Error>)` on initialization failures
///
/// # Errors
/// - Logging initialization failures
/// - File creation or timestamp update errors for individual files
/// - Invalid command-line arguments cause program termination
fn main() {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Error);

    let args = get_cli_arguments();

    validate_cli_arguments(&args);

    let mut success = true;
    for file in &args.files {
        if let Err(e) = touch_file(&file, &args) {
            error!("Error touching '{}': {}", file, e);
            success = false;
        }
    }

    if !success {
        exit_error();
    }

    exit_success();
}
