mod cli_utils;
mod models;
mod touch_app;

use crate::cli_utils::initialize;
use crate::touch_app::touch_file;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

/// Updates file access and modification times, creating files if they don't exist.
///
/// Mimics Unix `touch` command behavior by setting timestamps to the current time
/// or user-specified values. Processes multiple files sequentially.
fn main() {
    let args = match initialize() {
        Ok(a) => a,
        Err(e) => {
            error!("{}", e);
            exit_error();
        }
    };

    let mut success = true;
    for file in &args.files {
        if let Err(e) = touch_file(file, &args) {
            error!("Error touching '{}': {}", file, e);
            success = false;
        }
    }

    if success {
        exit_success();
    } else {
        exit_error();
    }
}