#![allow(dead_code, unused_imports)] // I'll remove this later.
use anyhow::Context;
use anyhow::Result;
use dotenv::dotenv;
use rusted_ai::agents::downloaded_media_sorter::cli_utils::print_runtime_info;
use rusted_ai::agents::downloaded_media_sorter::downloaded_media_sorter_app::handle_event_created;
use rusted_ai::constants::AI_DOWNLOADED_MEDIA_SORTER_NAME;
use rusted_ai::utils::monitor_folder::monitor_folder_for_new_files;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::setup_graceful_shutdown::setup_graceful_shutdown;
use std::env;
use tracing::{debug, error, info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    initialize_log(AI_DOWNLOADED_MEDIA_SORTER_NAME, LogLevel::Debug);

    println!("Rusted Agents: Media sorter");

    let _ = setup_graceful_shutdown(true);

    let folder_to_watch = env::var("AI_MEDIA_SORTER_WATCH_FOLDER")
        .context("AI_MEDIA_SORTER_WATCH_FOLDER is not set")?;

    print_runtime_info(&folder_to_watch);

    monitor_folder_for_new_files(folder_to_watch.as_str(), Some(handle_event_created)).await?;

    Ok(())
}
