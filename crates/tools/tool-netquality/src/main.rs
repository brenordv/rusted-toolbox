mod checks;
mod cli_utils;
mod models;
mod netqualify_app;
mod notifiers;
mod persistence;
mod runtime_state;

use crate::cli_utils::initialize;
use crate::netqualify_app::run_app;
use anyhow::Result;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

#[tokio::main]
async fn main() -> Result<()> {
    let (config, otel_guard) = match initialize().await {
        Ok(initialized) => initialized,
        Err(error) => {
            error!("Failed to initialize tool: {}", error);
            exit_error();
        }
    };

    let result = run_app(&config).await;

    // The exit helpers end the process without running Drop, so the OTel guard
    // is dropped here to flush buffered telemetry before any of them is called.
    drop(otel_guard);

    match result {
        Ok(_) => exit_success(),
        Err(error) => {
            eprintln!("{error}");
            exit_error();
        }
    };
}
