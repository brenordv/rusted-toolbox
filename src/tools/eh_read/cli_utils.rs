use crate::shared::command_line::cli_builder::CommandExt;
use crate::shared::constants::general::{DASH_LINE, EH_READ_APP_NAME};
use crate::shared::constants::versions::EH_READ_VERSION;
use crate::shared::eventhub::eventhub_models::EventHubConfig;
use crate::shared::eventhub::utils::cli_arguments::CommandCommonExt;
use clap::{ArgMatches, Command};

/// Displays EventHub Reader runtime configuration.
///
/// Shows entity path, consumer group, partition, database settings,
/// export options, checkpoint behavior, and feedback interval.
pub fn print_runtime_info(config: &mut EventHubConfig) {
    println!("🚀 EventHub Reader v{}", EH_READ_VERSION);
    println!("{}", DASH_LINE);
    println!("🎯 Entity Path: {}", config.entity_path);
    println!(
        "👥 Consumer Group: {}",
        config.inbound_config.consumer_group
    );
    println!(
        "📊 Partition: {}",
        config.inbound_config.get_partition_id_label()
    );
    println!("💾 Database: {}", config.inbound_config.database_path);
    println!(
        "📁 Base Data Folder: {}",
        config.inbound_config.base_data_folder
    );
    println!("🔊 Verbose: {}", config.verbose);
    println!("📄 Read to file: {}", config.inbound_config.read_to_file);
    if config.inbound_config.read_to_file {
        println!("📁 Export: {}", config.inbound_config.received_msg_path);
        println!(
            "📝 Content Only: {}",
            config.inbound_config.dump_content_only
        );
    }
    if config.inbound_config.ignore_checkpoint {
        println!(
            "⚠️ Ignore Checkpoint: {}",
            config.inbound_config.ignore_checkpoint
        );
    }
    if let Some(filters) = &config.inbound_config.dump_filter {
        let dump_filter_message = if !filters.is_empty() {
            filters.join(", ")
        } else {
            "No filter. All messages will be processed.".to_string()
        };
        println!("🔍 Dump Filter: {}", dump_filter_message);
    }
    println!(
        "⚡ Feedback: Every {} second(s)",
        &config.inbound_config.feedback_interval
    );
    println!();
}

/// Parses command-line arguments for EventHub reader configuration.
///
/// Supports JSON config file and CLI overrides for connection string,
/// entity path, consumer group, partition selection, and export options.
///
/// # Returns
/// Parsed command-line arguments as `ArgMatches`
pub fn get_cli_arguments() -> ArgMatches {
    Command::new(EH_READ_APP_NAME)
        .add_basic_metadata(
            EH_READ_VERSION,
            "Azure EventHub reader tool - reads messages from EventHub and stores them locally",
            "Azure EventHub reader tool - reads messages from EventHub and stores them locally.\n\n\
            All configuration options can be provided via JSON configuration file and/or command line arguments.\n\
            Command line arguments take precedence over JSON configuration values.")
        .preset_arg_verbose(None)
        .preset_arg_config(None)
        .preset_arg_connection_string("EventHub connection string")
        .add_eh_base_shared_args()
        .add_eh_reader_args()
        .get_matches()
}
