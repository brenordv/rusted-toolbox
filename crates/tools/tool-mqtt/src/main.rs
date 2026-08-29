use crate::cli_utils::initialize;
use crate::models::MqttCommand;
use crate::mqtt_app::{post_message, read_messages};
use anyhow::Result;

mod cli_utils;
mod models;
mod mqtt_app;

#[tokio::main]
async fn main() -> Result<()> {
    let args = initialize()?;

    match args.command {
        MqttCommand::Read => {
            read_messages(&args).await?;
        }
        MqttCommand::Post => {
            post_message(&args).await?;
        }
    }

    Ok(())
}
