use shared::constants::general::{EXIT_CODE_INTERRUPTED_BY_USER};
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::tool_exit_helpers::{exit_error, exit_success, exit_with_code};
use crate::cli_utils::{get_cli_arguments, print_runtime_info, validate_cli_arguments};
use crate::guid_app::{continuous_generation, copy_guid_to_clipboard, generate_once};
use tracing::error;

mod cli_utils;
mod models;
mod guid_app;

/// GUID generator tool.
///
/// Parses arguments, validates configuration, and generates GUIDs either once or continuously.
fn main() {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Info);

    let args = get_cli_arguments();

    validate_cli_arguments(&args);

    if !args.silent {
        print_runtime_info(&args);
    }

    if let Some(interval) = args.generate_on_interval {
        let _ = continuous_generation(interval, args.silent).inspect_err(|e| {
            error!("Error during continuous generation: {}", e);
            exit_error();
        });
        exit_with_code(EXIT_CODE_INTERRUPTED_BY_USER);
    } else {
        let guid = generate_once(args.generate_empty_guid);

        print!("{}", guid);

        if args.add_to_clipboard {
            copy_guid_to_clipboard(guid);
        }
    }

    exit_success();
}