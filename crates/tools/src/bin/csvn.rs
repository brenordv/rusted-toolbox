use rusted_toolbox::tools::csvn::cli_utils::{get_cli_arguments, print_runtime_info};
use rusted_toolbox::tools::csvn::csvn_app::process_file;
use rusted_toolbox::tools::csvn::models::CsvNConfig;
use shared::constants::general::CSVN_APP_NAME;
use shared::system::setup_graceful_shutdown::setup_graceful_shutdown;
use shared::system::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

/// CSV Normalizer tool entry point.
///
/// Normalizes CSV files by filling missing fields with default values.
/// Creates an output file with "_normalized" suffix.
///
/// # Exit Codes
/// - Success: 0
/// - Error: 1
fn main() {
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
