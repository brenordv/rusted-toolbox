mod cli_utils;
mod models;
mod chatbot_app;

use anyhow::Result;
use dotenv::dotenv;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::setup_graceful_shutdown::setup_graceful_shutdown;
use crate::chatbot_app::start_chatbot;
use crate::cli_utils::{get_runtime_config, print_runtime_info};

#[tokio::main]
async fn main() -> Result<()> {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Error);

    dotenv().ok();

    let _ = setup_graceful_shutdown(true);

    let runtime_config = get_runtime_config()?;

    print_runtime_info(&runtime_config);

    start_chatbot(runtime_config).await?;

    Ok(())
}