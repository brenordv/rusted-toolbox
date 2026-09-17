use crate::cli_utils::initialize;
use crate::models::MqttCommand;
use crate::mqtt_app::{post_message, read_messages};
use cli_signal_monitor::setup_graceful_shutdown::setup_graceful_shutdown;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

mod cli_utils;
mod models;
mod mqtt_app;

/// MQTT read/post tool.
///
/// Parses and validates arguments, installs the Ctrl+C handler, then reads
/// from or posts to the broker. Exit codes: 0 on success, 1 on failure.
#[tokio::main]
async fn main() {
    let args = match initialize() {
        Ok(a) => a,
        Err(e) => {
            error!("Invalid arguments: {:#}", e);
            exit_error();
        }
    };

    let shutdown_signal = setup_graceful_shutdown(false);

    let result = match args.command {
        MqttCommand::Read => read_messages(&args, shutdown_signal).await,
        MqttCommand::Post => post_message(&args, shutdown_signal).await,
    };

    match result {
        Ok(()) => exit_success(),
        Err(e) => {
            error!("{:#}", e);
            exit_error();
        }
    }
}
