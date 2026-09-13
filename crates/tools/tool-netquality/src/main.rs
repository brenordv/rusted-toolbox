mod checks;
mod cli_utils;
mod models;
mod netqualify_app;
mod notifiers;
mod persistence;
mod runtime_state;

use crate::cli_utils::initialize;
use crate::netqualify_app::run_app;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

#[tokio::main]
async fn main() {
    let (config, otel_guard) = match initialize().await {
        Ok(initialized) => initialized,
        Err(error) => {
            error!("Failed to initialize tool: {:#}", error);
            exit_error();
        }
    };

    // A failed run is logged while the OTel guard is still alive, so the
    // failure reaches the export pipeline before the guard's drop flushes and
    // shuts it down. The exit helpers end the process without running Drop,
    // which is why the guard is dropped explicitly before either is called.
    let succeeded = match run_app(&config).await {
        Ok(_) => true,
        Err(error) => {
            error!("{:#}", error);
            false
        }
    };

    drop(otel_guard);

    if succeeded {
        exit_success();
    } else {
        exit_error();
    }
}
