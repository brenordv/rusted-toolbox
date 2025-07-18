use log::error;
use rusted_toolbox::shared::system::setup_graceful_shutdown::setup_graceful_shutdown;
use rusted_toolbox::shared::system::tool_exit_helpers::{exit_error, exit_success};
use rusted_toolbox::tools::split::cli_utils::{
    ensure_cli_arguments_are_valid, get_cli_arguments, print_runtime_info,
};
use rusted_toolbox::tools::split::split_app::process_input_file;

/// File splitting tool with graceful shutdown support.
///
/// Parses CLI arguments, validates configuration, and processes input file.
/// Sets up signal handling for graceful termination during processing.
fn main() {
    let args = get_cli_arguments();

    ensure_cli_arguments_are_valid(&args);

    print_runtime_info(&args);

    // Set up a graceful shutdown
    let shutdown_signal = setup_graceful_shutdown();

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
