use log::error;
use rusted_toolbox::shared::constants::general::CAT_APP_NAME;
use rusted_toolbox::shared::logging::app_logger::LogLevel;
use rusted_toolbox::shared::logging::logging_helpers::initialize_log;
use rusted_toolbox::shared::system::tool_exit_helpers::{exit_error, exit_success};
use rusted_toolbox::tools::cat::cat_app::cat_file;
use rusted_toolbox::tools::cat::cli_utils::get_cli_arguments;
use rusted_toolbox::tools::cat::models::CatOptions;
use std::io::{self, Write};

/// Unix cat command implementation.
///
/// Concatenates files and prints to stdout. Reads from stdin if no files provided.
/// Supports line numbering, character visualization, and formatting options.
///
/// # Exit Codes
/// - 0: Success
/// - 1: Error occurred
fn main() {
    initialize_log(CAT_APP_NAME, LogLevel::Error);

    let args = get_cli_arguments();

    let options = CatOptions::from_args(&args);

    // Handle -u flag by setting stdout to unbuffered
    if args.u_flag {
        io::stdout().flush().unwrap_or(());
    }

    if args.files.is_empty() {
        // No files specified, read from stdin
        if let Err(e) = cat_file(None, &options) {
            error!("Failed to run CAT from stdin: {}", e);
            exit_error();
        }
    } else {
        // Process each file
        for filename in &args.files {
            if let Err(e) = cat_file(Some(filename), &options) {
                error!("Failed to run CAT from file [{}]: {}", filename, e);
                exit_error();
            }
        }
    }

    exit_success();
}
