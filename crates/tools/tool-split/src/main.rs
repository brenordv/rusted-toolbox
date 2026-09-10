mod cli_utils;
mod models;
mod split_app;

use crate::cli_utils::initialize;
use crate::split_app::{process_input_file, RunOutcome};
use cli_signal_monitor::setup_graceful_shutdown::setup_graceful_shutdown;
use common_cli::tool_exit_helpers::{exit_error, exit_success, exit_with_code};
use common_utils::constants::EXIT_CODE_INTERRUPTED_BY_USER;
use tracing::error;

/// File splitting tool with graceful shutdown support.
///
/// Parses and validates CLI arguments, installs the Ctrl+C handler, and splits
/// the input file. Exit codes: 0 on success, 1 on failure, 130 when
/// interrupted.
fn main() {
    let args = match initialize() {
        Ok(a) => a,
        Err(e) => {
            // Logging is not installed yet at this point, so report on stderr directly.
            eprintln!("split failed to start: {}", e);
            exit_error();
        }
    };

    let shutdown_signal = setup_graceful_shutdown(false);

    match process_input_file(&args, shutdown_signal) {
        Ok(RunOutcome::Completed) => exit_success(),
        Ok(RunOutcome::Interrupted) => exit_with_code(EXIT_CODE_INTERRUPTED_BY_USER),
        Err(e) => {
            error!("Error splitting input file: {}", e);
            exit_error();
        }
    }
}
