use crate::cli_utils::initialize;
use crate::models::MqttCommand;
use crate::mqtt_app::{post_message, read_messages};
use anyhow::Result;
use cli_signal_monitor::setup_graceful_shutdown::setup_graceful_shutdown;

mod cli_utils;
mod models;
mod mqtt_app;

#[tokio::main]
async fn main() -> Result<()> {
    let args = initialize()?;
    let shutdown_signal = setup_graceful_shutdown(false);

    match args.command {
        MqttCommand::Read => {
            read_messages(&args, shutdown_signal).await?;
        }
        MqttCommand::Post => {
            post_message(&args, shutdown_signal).await?;
        }
    }

    Ok(())
}
