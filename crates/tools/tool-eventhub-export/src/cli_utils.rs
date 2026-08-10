use clap::{ArgMatches, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;
use shared_eventhub::eventhub_models::EventHubConfig;
use shared_eventhub::utils::cli_arguments::CommandCommonExt;

/// Displays runtime configuration and settings for the EventHub Exporter.
///
/// # Arguments
/// - `config`: EventHub configuration to display
///
/// # Output
/// - Version, database paths, export settings, filters, and operational flags
/// - Formatted with emojis for visual clarity
pub fn print_runtime_info(config: &mut EventHubConfig) {
    println!("EventHub Exporter v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);
    println!("- Verbose: {}", config.verbose);
    println!("- Source Database: {}", config.inbound_config.database_path);
    println!(
        "- Export Checkpoint Database: {}",
        config.export_config.database_path
    );
    println!(
        "- Export Base Folder: {}",
        config.export_config.base_data_folder
    );
    println!("- Export Format: {}", config.export_config.export_format);
    println!(
        "- Condense Output: {}",
        config.export_config.condense_output
    );
    println!(
        "- Include Metadata: {}",
        config.export_config.include_metadata
    );
    println!(
        "- Ignore Checkpoint: {}",
        config.export_config.ignore_checkpoint
    );
    if let Some(filters) = &config.export_config.dump_filter {
        let dump_filter_message = if !filters.is_empty() {
            filters.join(", ")
        } else {
            "No filter. All messages will be processed.".to_string()
        };
        println!("- Dump Filter: {}", dump_filter_message);
    }
    println!(
        "- Feedback: Every {} second(s)",
        &config.inbound_config.feedback_interval
    );
    println!(
        "- Ignore Checkpoint: {}",
        config.export_config.ignore_checkpoint
    );
    println!("- Use Local Time: {}", config.export_config.use_local_time);
    println!("- Binary data will be converted to base64 if encountered");
    println!();
}

/// Parses command-line arguments for the Azure EventHub export tool.
///
/// # Returns
/// - `ArgMatches`: Parsed command-line arguments and values
///
/// # Features
/// - JSON configuration file support with CLI override capability
/// - Shared EventHub connection arguments
/// - Export-specific arguments for format, filters, and output options
pub fn get_cli_arguments() -> ArgMatches {
    Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            "Azure EventHub export tool - exports messages from local database to files",
            "Azure EventHub export tool - exports messages from local database to files.\n\n\
            This tool reads messages previously saved by eh-read and exports them to various formats.\n\
            All configuration options can be provided via JSON configuration file and/or command line arguments.\n\
            Command line arguments take precedence over JSON configuration values.")
        .preset_arg_verbose(None)
        .preset_arg_config(None)
        .add_eh_base_shared_args()
        .add_eh_export_args()
        .get_matches()
}
