#![allow(dead_code, unused_imports)] // I'll remove this later.
use anyhow::Context;
use anyhow::Result;
use dotenv::dotenv;
use rusted_toolbox::shared::logging::app_logger::LogLevel;
use rusted_toolbox::shared::logging::logging_helpers::initialize_log;
use rusted_toolbox::tools::ai::ai_agent_media_sorter_app::handle_event_created;
use rusted_toolbox::tools::ai::ai_functions::media_sorter_functions::{
    extract_episode_title_from_filename_as_string, extract_movie_title_from_filename_as_string,
    extract_season_episode_from_filename_as_string, extract_tv_show_title_from_filename_as_string,
    identify_media_format_from_filename_as_string, identify_media_type_from_filename,
    identify_media_type_from_filename_as_string, is_main_archive_file_as_string,
};
use rusted_toolbox::tools::ai::message_builders::system_message_builders::{
    build_rust_ai_function_system_message, build_rust_ai_function_user_message,
};
use rusted_toolbox::tools::ai::requesters::requester_builders::{
    build_requester_for_open_router, build_requester_for_openai, build_requester_for_openwebui,
};
use rusted_toolbox::tools::ai::requesters::requester_traits::OpenAiRequesterTraits;
use rusted_toolbox::tools::ai::utils::monitor_folder::monitor_folder_for_new_files;
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
