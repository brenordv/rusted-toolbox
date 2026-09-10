mod cli_utils;
mod models;
mod rxget_app;
mod targets;

use crate::cli_utils::initialize;
use crate::models::RunOutcome;
use crate::rxget_app::run;
use cli_signal_monitor::setup_graceful_shutdown::setup_graceful_shutdown;
use common_cli::tool_exit_helpers::{exit_error, exit_success, exit_with_code};
use common_utils::constants::{EXIT_CODE_INTERRUPTED_BY_USER, SIZE_128KB};
use std::io::{self, BufWriter};
use tracing::error;

/// Maps the run to the exit contract: 0 completed (even with zero values
/// extracted), 1 any target error or per-file failure, 130 interrupted
/// (interruption wins over an earlier failure).
fn main() {
    let (config, expansion_failed) = initialize();

    let shutdown = setup_graceful_shutdown(false);
    let stdout = io::stdout();
    let mut out = BufWriter::with_capacity(SIZE_128KB, stdout.lock());

    match run(&config, &shutdown, &mut out) {
        Ok(result) => {
            if result.outcome == RunOutcome::Interrupted {
                exit_with_code(EXIT_CODE_INTERRUPTED_BY_USER);
            }
            if result.all_ok && !expansion_failed {
                exit_success();
            }
            exit_error();
        }
        Err(run_error) => {
            error!("{:#}", run_error);
            exit_error();
        }
    }
}
