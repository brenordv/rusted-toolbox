mod cli_utils;
mod image_app;
mod image_edit_routines;
mod image_encoders;
mod image_format_traits;
mod models;
mod string_traits;

use crate::cli_utils::initialize;
use crate::image_app::run_image_edit_commands;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

/// Image editing tool (resize, grayscale, convert).
///
/// Parses arguments, then runs every edit job; any failed job makes the run
/// fail. Exit codes: 0 on success, 1 when any job or the setup fails.
fn main() {
    let args = initialize();

    match run_image_edit_commands(&args) {
        Ok(()) => exit_success(),
        Err(e) => {
            error!("{:#}", e);
            exit_error();
        }
    }
}
