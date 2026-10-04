mod cli_utils;
mod models;
mod seal_app;

use crate::cli_utils::initialize;
use crate::seal_app::{RunOutcome, run};
use cli_signal_monitor::setup_graceful_shutdown::setup_graceful_shutdown;
use common_cli::broken_pipe::BrokenPipe;
use common_cli::tool_exit_helpers::{exit_error, exit_success, exit_with_code};
use common_utils::constants::EXIT_CODE_INTERRUPTED_BY_USER;
use tracing::{debug, error};

/// File encryption tool over the age format.
///
/// Parses arguments, installs the Ctrl+C handler, and runs the selected
/// subcommand. Exit codes: 0 on success (including a consumer closing the
/// output pipe early), 1 on any failure, 130 when interrupted.
fn main() {
    let command = match initialize() {
        Ok(command) => command,
        Err(e) => {
            error!("{:#}", e);
            exit_error();
        }
    };

    let shutdown = setup_graceful_shutdown(false);

    match run(command, &shutdown) {
        Ok(RunOutcome::Completed) => exit_success(),
        Ok(RunOutcome::Interrupted) => exit_with_code(EXIT_CODE_INTERRUPTED_BY_USER),
        Err(e) if e.is::<BrokenPipe>() => {
            debug!("stopping early: output pipe closed by the consumer");
            exit_success();
        }
        Err(e) => {
            error!("{:#}", e);
            exit_error();
        }
    }
}
