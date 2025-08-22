mod chatbot_app;
mod cli_utils;
mod models;

use crate::chatbot_app::start_chatbot;
use crate::cli_utils::{get_runtime_config, print_runtime_info};
use anyhow::Result;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::load_env_variables::load_env_variables;
use shared::system::setup_graceful_shutdown::setup_graceful_shutdown;

#[tokio::main]
async fn main() -> Result<()> {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Error);

    load_env_variables()?;

    let _ = setup_graceful_shutdown(true);

    let runtime_config = get_runtime_config()?;

    print_runtime_info(&runtime_config);

    start_chatbot(runtime_config).await?;

    Ok(())
}
