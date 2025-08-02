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
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<()> {
    initialize_log(APP_SMO_WATCHDOG_SLUG, LogLevel::Debug);

    let runtime_config = get_runtime_info()?;

    print_runtime_info(&runtime_config);

    let decompression_task_file_processor = ProcessFileRoutine::new(&runtime_config)?;

    // Channel: file watcher sends the main-archive file full path to be decompressed.
    let (tx, mut rx) = mpsc::channel::<String>(100);

    let file_processor = ProcessFileRoutine::new_with_channel(&runtime_config, tx)?;
    let watcher_shared_file_processor = Arc::from(&file_processor);

    // Channel data receiver
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(main_path_str) = rx.recv() => {
                    let _ = decompression_task_file_processor.handle_decompress_file(main_path_str);
                }

                // tick to drive debounce
                _ = sleep(Duration::from_millis(1000)) => {

                }
            }
        }
    });

    // This approach of creating a clojure and using Arc to share the file processor works here
    // because we're not doing anything in parallel. However, if we were, this would probably fail
    // catastrophically because the SQLite engine is not meant to be shared across threads.
    let handler = move |event, event_type, path| {
        let processor = watcher_shared_file_processor.clone();
        async move {
            processor
                .handle_on_file_created(event, event_type, path)
                .await
        }
    };

    monitor_folder_for_on_created_only(&runtime_config.get_watch_folder(), handler).await?;

    Ok(())
}
