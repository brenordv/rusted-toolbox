use anyhow::Result;
use scene_media_organizer::constants::APP_SMO_WATCHDOG_SLUG;
use scene_media_organizer::tools::smo_watchdog_app::cli_utils::{get_runtime_info, print_runtime_info};
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::monitor_folder::{dummy_handle_event_created, monitor_folder_for_on_created_only};

#[tokio::main]
async fn main() -> Result<()> {
    initialize_log(APP_SMO_WATCHDOG_SLUG, LogLevel::Debug);

    let runtime_config = get_runtime_info()?;

    print_runtime_info(&runtime_config);

    monitor_folder_for_on_created_only(
        &runtime_config.get_watch_folder(),
        dummy_handle_event_created
    ).await?;

    Ok(())
}