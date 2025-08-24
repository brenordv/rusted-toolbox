use crate::cli_utils::{get_cli_arguments, print_runtime_info, validate_args};
use crate::models::MqttCommand;
use crate::mqtt_app::{post_message, read_messages};
use anyhow::Result;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;

mod cli_utils;
mod models;
mod mqtt_app;
mod string_traits;

#[tokio::main]
async fn main() -> Result<()> {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Info);

    let args = get_cli_arguments()?;
    validate_args(&args)?;
    print_runtime_info(&args);

    match args.command {
        MqttCommand::Unknown => {}
        MqttCommand::Read => {
            read_messages(&args).await?;
        }
        MqttCommand::Post => {
            post_message(&args).await?;
        }
    }

    Ok(())
}
