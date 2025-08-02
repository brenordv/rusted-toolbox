use anyhow::Result;
use scene_media_organizer::constants::APP_SMO_WATCHDOG_SLUG;
use scene_media_organizer::routines::process_file::ProcessFileRoutine;
use scene_media_organizer::tools::smo_watchdog_app::cli_utils::{
    get_runtime_info, print_runtime_info,
};
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::monitor_folder::monitor_folder_for_on_created_only;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    initialize_log(APP_SMO_WATCHDOG_SLUG, LogLevel::Debug);

    let runtime_config = get_runtime_info()?;

    print_runtime_info(&runtime_config);

    let file_processor = ProcessFileRoutine::new(
        runtime_config.db_data_file.clone(),
        runtime_config.guess_it_api_base_url.clone(),
        runtime_config.unrar_bin_path.clone(),
    )?;

    let shared_processor = Arc::from(&file_processor);

    // This approach of creating a clojure and using Arc to share the file processor works here
    // because we're not doing anything in parallel. However, if we were, this would probably fail
    // catastrophically because the SQLite engine is not meant to be shared across threads.
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
