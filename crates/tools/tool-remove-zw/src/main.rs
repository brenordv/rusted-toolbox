mod cli_utils;
mod models;
mod remove_zw_app;

use crate::cli_utils::initialize;
use crate::remove_zw_app::run;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

fn main() {
    let args = match initialize() {
        Ok(a) => a,
        Err(e) => {
            error!("Failed to parse arguments: {}", e);
            exit_error();
            unreachable!();
        }
    };

    match run(&args) {
        Ok(()) => exit_success(),
        Err(e) => {
            error!("Failed to remove zero-width characters: {}", e);
            exit_error();
        }
    }
}
