use crate::cli_utils::initialize;
use crate::csvn_app::process_file;
use crate::models::CsvNConfig;
use cli_signal_monitor::setup_graceful_shutdown::setup_graceful_shutdown;
use common_cli::tool_exit_helpers::{exit_error, exit_success, exit_with_code};
use common_utils::constants::EXIT_CODE_INTERRUPTED_BY_USER;
use tracing::error;

mod cli_utils;
mod csvn_app;
mod models;

fn main() {
    const CSVN_APP_NAME: &str = env!("CARGO_PKG_NAME");

    let args: CsvNConfig = match initialize() {
        Ok(a) => a,
        Err(e) => {
            // Logging is not installed yet at this point, so report on stderr directly.
            error!("{} failed to parse arguments: {}", CSVN_APP_NAME, e);
            exit_error();;
        }
    };

    // Set up a graceful shutdown
    let shutdown_signal = setup_graceful_shutdown(false);

    match process_file(&args, shutdown_signal) {
        Ok(interrupted) => {
            if interrupted {
                exit_with_code(EXIT_CODE_INTERRUPTED_BY_USER);
            } else {
                exit_success();
            }
        }
        Err(e) => {
            error!("{} failed to execute: {}", CSVN_APP_NAME, e);
            exit_error();
        }
    }
}