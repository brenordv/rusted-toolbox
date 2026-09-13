mod cli_utils;
mod models;
mod pingx_app;

use crate::cli_utils::initialize;
use crate::pingx_app::run_ping;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

#[tokio::main]
async fn main() {
    let args = match initialize() {
        Ok(a) => a,
        Err(e) => {
            error!("Invalid arguments: {:#}", e);
            exit_error();
        }
    };

    match run_ping(&args).await {
        Ok(()) => exit_success(),
        Err(e) => {
            error!("Failed to ping target: {}", e);
            exit_error();
        }
    }
}
