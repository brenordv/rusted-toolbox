use anyhow::Result;
use dotenv::dotenv;
use rusted_ai::agents::chatbot::chatbot_app::start_chatbot;
use rusted_ai::agents::chatbot::cli_utils::{get_runtime_config, print_runtime_info};
use shared::system::setup_graceful_shutdown::setup_graceful_shutdown;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    let _ = setup_graceful_shutdown(true);

    let runtime_config = get_runtime_config()?;

    print_runtime_info(&runtime_config);

    start_chatbot(runtime_config).await?;

    Ok(())
}
