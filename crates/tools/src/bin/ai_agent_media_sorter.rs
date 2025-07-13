use anyhow::Context;
use dotenv::dotenv;
use rusted_toolbox::tools::ai::ai_agent_media_sorter_app::handle_file_created;
use rusted_toolbox::tools::ai::utils::monitor_folder::monitor_folder_for_new_files;
use std::env;

fn main() -> anyhow::Result<()> {
    dotenv().ok();

    println!("Rusted Agents: Media sorter");

    let folder_to_watch = env::var("AI_MEDIA_SORTER_WATCH_FOLDER")
        .context("AI_MEDIA_SORTER_WATCH_FOLDER is not set")?;

    monitor_folder_for_new_files(folder_to_watch.as_str(), Some(handle_file_created))?;

    Ok(())
}
