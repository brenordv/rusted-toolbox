use anyhow::Result;
use notify::Event;
use scene_media_organizer::constants::APP_SMO_WATCHDOG_SLUG;
use scene_media_organizer::routines::process_file::ProcessFileRoutine;
use scene_media_organizer::tools::smo_watchdog_app::cli_utils::{
    get_runtime_info, print_runtime_info,
};
use scene_media_organizer::utils::guessit_client::GuessItClient;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::monitor_folder::{monitor_folder_for_on_created_only, EventType};
use std::path::PathBuf;
use std::sync::Arc;
use tracing_log::log::debug;

pub async fn test(event: Event, event_type: EventType, _: PathBuf) -> Result<()> {
    let created_entries = &event.paths;
    debug!(
        "[{:?}] On Created Event (count: {}): [{:?}]",
        event_type,
        created_entries.len(),
        event
    );

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

    let file_processor = ProcessFileRoutine::new(
        ".data/watchdog.db".to_string(),
        "http://rverse.local:10147/api/guess".to_string(),
    )?;

    let shared_processor = Arc::from(&file_processor);

    let handler = move |event, event_type, path| {
        let processor = shared_processor.clone();
        async move {
            processor
                .handle_on_file_created(event, event_type, path)
                .await
        }
    };

    monitor_folder_for_on_created_only(&runtime_config.get_watch_folder(), handler).await?;

    Ok(())
}
