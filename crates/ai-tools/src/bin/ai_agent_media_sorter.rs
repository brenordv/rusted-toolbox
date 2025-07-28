#![allow(dead_code, unused_imports)] // I'll remove this later.
use anyhow::Context;
use anyhow::Result;
use dotenv::dotenv;
use rusted_ai::ai_agent_media_sorter_app::handle_event_created;
use rusted_ai::utils::monitor_folder::monitor_folder_for_new_files;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use std::env;
use tracing::{debug, error, info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    initialize_log("MEDIA_SORTER", LogLevel::Debug);
    dotenv().ok();

    println!("Rusted Agents: Media sorter");

    let folder_to_watch = env::var("AI_MEDIA_SORTER_WATCH_FOLDER")
        .context("AI_MEDIA_SORTER_WATCH_FOLDER is not set")?;

    monitor_folder_for_new_files(folder_to_watch.as_str(), Some(handle_event_created)).await?;

    Ok(())
}
