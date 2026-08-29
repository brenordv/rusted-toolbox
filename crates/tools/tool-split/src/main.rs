mod cli_utils;
mod models;
mod split_app;

use crate::cli_utils::initialize;
use crate::split_app::process_input_file;
use cli_signal_monitor::setup_graceful_shutdown::setup_graceful_shutdown;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

/// File splitting tool with graceful shutdown support.
///
/// Parses and validates CLI arguments, installs the Ctrl+C handler, and splits the input file.
fn main() {
    let args = match initialize() {
        Ok(a) => a,
        Err(e) => {
            error!("Failed to parse arguments: {}", e);
            exit_error();
            unreachable!();
        }
    };

    let shutdown_signal = setup_graceful_shutdown(false);

    match process_input_file(&args, shutdown_signal) {
        Ok(_) => exit_success(),
        Err(e) => {
            error!("Error splitting input file: {}", e);
            exit_error();
        }
    }
}
