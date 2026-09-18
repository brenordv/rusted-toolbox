use crate::cli_utils::initialize;
use crate::get_lines_app::{RunOutcome, run};
use cli_signal_monitor::setup_graceful_shutdown::setup_graceful_shutdown;
use common_cli::tool_exit_helpers::{exit_error, exit_success, exit_with_code};
use common_utils::constants::EXIT_CODE_INTERRUPTED_BY_USER;
use tracing::error;

mod cli_utils;
mod get_lines_app;
mod models;

/// Entry point for the get-lines tool.
///
/// Parses arguments, installs the Ctrl+C handler, runs the single-pass search, and maps the
/// outcome to a process exit code: `0` on completion, `1` on failure, `130` on interruption.
fn main() {
    let config = initialize();

    let shutdown_signal = setup_graceful_shutdown(false);

    match run(&config, shutdown_signal) {
        Ok(RunOutcome::Completed) => exit_success(),
        Ok(RunOutcome::Interrupted) => exit_with_code(EXIT_CODE_INTERRUPTED_BY_USER),
        Err(error) => {
            error!("{:#}", error);
            exit_error();
        }
    }
}
