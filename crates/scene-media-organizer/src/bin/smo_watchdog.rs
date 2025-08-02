use std::path::PathBuf;
use std::time::Duration;
use anyhow::Result;
use notify::Event;
use tracing_log::log::debug;
use scene_media_organizer::constants::APP_SMO_WATCHDOG_SLUG;
use scene_media_organizer::tools::smo_watchdog_app::cli_utils::{get_runtime_info, print_runtime_info};
use scene_media_organizer::utils::guessit_client::GuessItClient;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::monitor_folder::{dummy_handle_event_created, monitor_folder_for_on_created_only, EventType};
use shared::system::pathbuf_extensions::PathBufExtensions;

pub async fn test(event: Event, event_type: EventType, _: PathBuf) -> Result<()> {
    let created_entries = &event.paths;
    debug!("[{:?}] On Created Event (count: {}): [{:?}]", event_type, created_entries.len(), event);

    let guess = GuessItClient::new("http://rverse.local:10147/api/guess".parse()?);
    
    for entry in created_entries {
        debug!("Processing file: {:?}", entry);
        
        let response = guess.it(entry.to_str().unwrap().to_string()).await?;
        
        debug!("Response: {:?}", response);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    initialize_log(APP_SMO_WATCHDOG_SLUG, LogLevel::Debug);

    let runtime_config = get_runtime_info()?;

    print_runtime_info(&runtime_config);

    monitor_folder_for_on_created_only(
        &runtime_config.get_watch_folder(),
        test
    ).await?;

    Ok(())
}