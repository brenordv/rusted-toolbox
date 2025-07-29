use anyhow::Result;
use dotenv::dotenv;
use rusted_ai::agents::chatbot::chatbot_app::start_chatbot;
use rusted_ai::agents::chatbot::cli_utils::{get_runtime_config, print_runtime_info};
use rusted_ai::constants::AI_CHATBOT_NAME;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::setup_graceful_shutdown::setup_graceful_shutdown;

#[tokio::main]
async fn main() -> Result<()> {
    initialize_log(AI_CHATBOT_NAME, LogLevel::Error);

    dotenv().ok();

    let _ = setup_graceful_shutdown(true);

    let runtime_config = get_runtime_config()?;

    print_runtime_info(&runtime_config);

    start_chatbot(runtime_config).await?;

    Ok(())
}