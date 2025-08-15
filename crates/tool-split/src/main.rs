use shared::system::setup_graceful_shutdown::setup_graceful_shutdown;
use shared::system::tool_exit_helpers::{exit_error, exit_success};
use crate::cli_utils::{ensure_cli_arguments_are_valid, get_cli_arguments, print_runtime_info};
use crate::split_app::process_input_file;
use tracing::{error};

mod cli_utils;
mod models;
mod split_app;

/// File splitting tool with graceful shutdown support.
///
/// Parses CLI arguments, validates configuration, and processes input file.
/// Sets up signal handling for graceful termination during processing.
fn main() {
    let args = get_cli_arguments();

    ensure_cli_arguments_are_valid(&args);

    print_runtime_info(&args);

    let shutdown_signal = setup_graceful_shutdown(false);

    match process_input_file(&args, shutdown_signal) {
        Ok(_) => {
            exit_success();
        }
        Err(e) => {
            error!("Error splitting input file: {}", e);
            exit_error();
        }
    }
}