use shared::system::setup_graceful_shutdown::setup_graceful_shutdown;
use shared::system::tool_exit_helpers::{exit_error, exit_success};
use crate::cli_utils::{get_cli_arguments, print_runtime_info};
use crate::models::CsvNConfig;
use tracing::error;
use crate::csvn_app::process_file;

mod models;
mod cli_utils;
mod csvn_app;

fn main() {
    const CSVN_APP_NAME: &str = env!("CARGO_PKG_NAME");
    let mut args: CsvNConfig;

    match get_cli_arguments() {
        Ok(a) => args = a,
        Err(e) => {
            error!("{} failed to parse arguments: {}", CSVN_APP_NAME, e);
            exit_error();

            // The `exit_error()` ends the program, the return statement makes the compiler happy.
            return;
        }
    }

    print_runtime_info(&args);

    // Set up a graceful shutdown
    let shutdown_signal = setup_graceful_shutdown(false);

    match process_file(&mut args, shutdown_signal) {
        Ok(_) => exit_success(),
        Err(e) => {
            error!("{} failed to execute: {}", CSVN_APP_NAME, e);
            exit_error();
        }
    }
}