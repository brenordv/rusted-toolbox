mod cli_utils;
mod models;
mod ts_app;

use crate::cli_utils::initialize;
use crate::ts_app::process_input;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

/// Main entry point for the timestamp converter tool.
///
/// Parses CLI arguments and converts between Unix timestamps and datetimes in either direction.
fn main() {
    let args = initialize();

    match process_input(&args.input) {
        Ok(_) => exit_success(),
        Err(e) => {
            error!("Error: {}", e);
            exit_error();
        }
    }
}
