use crate::cli_utils::{get_cli_arguments, print_runtime_info};
use crate::eventhub_reader_app::EventHubReader;
use crate::graceful_shutdown::{graceful_shutdown_routine, setup_graceful_shutdown};
use crate::runtime_config_utils::{apply_cli_overrides, validate_config};
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::get_current_working_dir::get_current_working_dir;
use shared::system::tool_exit_helpers::{exit_error, exit_success};
use shared_eventhub::utils::config_utils::get_base_config_object;
use tracing::error;

mod cli_utils;
mod eventhub_reader_app;
mod graceful_shutdown;
mod progress_tracker;
mod runtime_config_utils;

/// EventHub message reader with checkpoint/resume support.
///
/// Initializes logging, parses CLI arguments, loads configuration,
/// validates settings, creates EventHub consumer, and processes messages
/// until completion or shutdown signal.
///
/// # Errors
/// Returns error if configuration is invalid, EventHub connection fails,
/// or message processing encounters unrecoverable errors.
#[tokio::main]
async fn main() {
    // Initialize logging for the app
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Info);

    // Get CLI arguments
    let matches = get_cli_arguments();

    // Load config from the JSON file
    let current_dir = get_current_working_dir();
    let mut config = get_base_config_object(&matches, &current_dir)
        .await
        .inspect_err(|e| {
            error!("Failed to load config from the file: {}", e);
            exit_error();
        })
        .unwrap();

    // Update config loaded from the file with the CLI arguments
    let _ = apply_cli_overrides(&mut config, &matches, &current_dir).inspect_err(|e| {
        error!("Failed to apply CLI overrides: {}", e);
        exit_error();
    });

    // Validate config
    let _ = validate_config(&config).inspect_err(|e| {
        error!("Invalid configuration file detected: [{}]", e);
        exit_error();
    });

    // Print the runtime info so that the user knows what is going on
    print_runtime_info(&mut config);

    // Create EventHub reader Instance
    let mut reader = EventHubReader::new(config)
        .await
        .inspect_err(|e| {
            error!("Failed to create EventHub reader: {}", e);
            exit_error();
        })
        .unwrap();

    // The setup graceful shutdown
    let _ = setup_graceful_shutdown(&mut reader).inspect_err(|e| {
        error!("Failed to setup graceful shutdown: {}", e);
        exit_error();
    });

    // Start reading the messages and wait until it finishes, an error occurs,
    // or the user presses Ctrl+C
    let result = reader.start_reading().await;

    // Final cleanup with the graceful shutdown and timeout
    println!("[OK]  Cleaning up resources...");

    // Use a timeout for graceful shutdown to prevent hanging forever
    let _ = graceful_shutdown_routine(reader, result)
        .await
        .inspect_err(|e| {
            error!("Failed to gracefully shutdown: {}", e);
            exit_error();
        });

    exit_success();
}
