use crate::cli_utils::{get_cli_arguments, print_runtime_info};
use crate::eventhub_export_app::EventHubExporter;
use crate::runtime_config_utils::{apply_cli_overrides, validate_config};
use shared::constants::general::EXIT_CODE_INTERRUPTED_BY_USER;
use shared::eventhub::utils::config_utils::get_base_config_object;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::get_current_working_dir::get_current_working_dir;
use shared::system::setup_graceful_shutdown::setup_graceful_shutdown;
use shared::system::tool_exit_helpers::{exit_error, exit_success, exit_with_code};
use std::sync::Arc;
use tracing::{error, info};

mod cli_utils;
mod eventhub_export_app;
mod export_progress_tracker;
mod message_exporters;
mod runtime_config_utils;

/// Azure EventHub export tool - exports messages from local database to files.
///
/// Reads messages previously saved by eh-read and exports them to various formats (TXT, CSV, JSON).
/// Configuration via JSON file and/or command-line arguments, with CLI taking precedence.
///
/// # Workflow
/// 1. Initialize logging and parse CLI arguments
/// 2. Load and validate configuration with CLI overrides
/// 3. Set up graceful shutdown handling
/// 4. Create EventHubExporter and start export process
/// 5. Handle success, interruption, or error scenarios
///
/// # Exit Codes
/// - 0: Export completed successfully
/// - 130: Export interrupted by user (SIGINT)
/// - 1: Export failed due to error
///
/// # Dependencies
/// - `tokio::main` macro for asynchronous runtime.
/// - `anyhow` crate for error handling.
/// - Logging tools (likely provided by `tracing` crate).
/// - Custom modules for functions like `initialize_log`, `get_cli_arguments`, `get_base_config_object`, etc.
///
/// # Panics
/// The function will panic in the following scenarios:
/// - Failing to load a valid configuration.
/// - Errors during graceful shutdown setup (e.g., signal handler registration).
///
/// # Example
/// Run the application:
/// ```bash
/// cargo run -- --config /path/to/config.json
/// ```
///
/// Upon execution, it processes the configuration, validates it, and starts exporting data,
/// handling possible user interruptions or errors during the process.
#[tokio::main]
async fn main() {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Info);

    // Get CLI arguments
    let matches = get_cli_arguments();

    // Get the current working directory for relative paths
    let current_dir = get_current_working_dir();

    // Load configuration
    let mut config = get_base_config_object(&matches, &current_dir)
        .await
        .inspect_err(|e| {
            error!("Failed to load base configuration: [{}]", e);
            exit_error();
        })
        .unwrap();

    // Override config with CLI arguments
    apply_cli_overrides(&mut config, &matches, &current_dir)
        .inspect_err(|e| {
            error!("Failed to apply CLI overrides: [{}]", e);
            exit_error();
        })
        .unwrap();

    // Validate required configuration
    match validate_config(&config) {
        Ok(()) => info!("Configuration is valid"),
        Err(e) => {
            error!("Configuration is invalid: [{}]", e);
            exit_error();
        }
    };

    // Print startup information
    print_runtime_info(&mut config);

    // Set up a graceful shutdown
    let shutdown = setup_graceful_shutdown(false);

    // Create an exporter and start
    let exporter = EventHubExporter::new(config, Some(Arc::clone(&shutdown)))
        .await
        .inspect_err(|e| {
            error!("Failed to create exporter: {}", e);
            exit_error();
        })
        .unwrap();

    match exporter.start_export().await {
        Ok(()) => {
            println!("🎉 Export completed successfully!");
            exit_success();
            return;
        }
        Err(_e) if shutdown.load(std::sync::atomic::Ordering::Relaxed) => {
            exporter.shutdown();
            println!("⚠️ Export interrupted by user");
            exit_with_code(EXIT_CODE_INTERRUPTED_BY_USER);
            return;
        }
        Err(e) => {
            error!("Export failed: {}", e);
            exit_error();
            return;
        }
    };
}
