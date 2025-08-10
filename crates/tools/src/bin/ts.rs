use rusted_toolbox::tools::ts::cli_utils::{get_cli_arguments, print_runtime_info};
use rusted_toolbox::tools::ts::ts_app::process_input;
use shared::constants::general::TS_APP_NAME;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

/// Main entry point for the timestamp converter tool.
///
/// Initializes logging, parses CLI arguments, displays runtime info, and processes the input.
/// Handles both Unix timestamp to datetime conversion and datetime to Unix timestamp conversion.
fn main() {
    initialize_log(TS_APP_NAME, LogLevel::Error);

    let args = get_cli_arguments();

    print_runtime_info(&args);

    match process_input(&args.input) {
        Ok(_) => {
            exit_success();
        }
        Err(e) => {
            error!("Error: {}", e);
            exit_error();
        }
    }
}
