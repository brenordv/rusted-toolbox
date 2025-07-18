use rusted_toolbox::shared::constants::general::{
    EH_EXPORT_APP_NAME, EXIT_CODE_INTERRUPTED_BY_USER,
};
use rusted_toolbox::shared::eventhub::utils::config_utils::get_base_config_object;
use rusted_toolbox::shared::logging::app_logger::LogLevel;
use rusted_toolbox::shared::logging::logging_helpers::initialize_log;
use rusted_toolbox::shared::system::get_current_working_dir::get_current_working_dir;
use rusted_toolbox::shared::system::setup_graceful_shutdown::setup_graceful_shutdown;
use rusted_toolbox::shared::system::tool_exit_helpers::{exit_error, exit_success, exit_with_code};
use rusted_toolbox::tools::eh_export::cli_utils::{get_cli_arguments, print_runtime_info};
use rusted_toolbox::tools::eh_export::eventhub_export_app::EventHubExporter;
use rusted_toolbox::tools::eh_export::runtime_config_utils::{
    apply_cli_overrides, validate_config,
};
use std::sync::Arc;
use tracing::{error, info};

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
    initialize_log(EH_EXPORT_APP_NAME, LogLevel::Info);

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
    let shutdown = setup_graceful_shutdown();

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
