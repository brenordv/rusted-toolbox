use anyhow::{anyhow, Result};
use clap::ArgMatches;
use shared::eventhub::eventhub_models::EventHubConfig;
use std::path::PathBuf;

/// Applies CLI argument overrides to EventHub configuration.
///
/// Updates config with command-line values for connection string, paths,
/// consumer settings, partition selection, and export options.
/// Resolves relative paths to absolute paths using current directory.
///
/// # Errors
/// Returns error if path resolution fails or validation errors occur.
pub fn apply_cli_overrides(
    config: &mut EventHubConfig,
    matches: &ArgMatches,
    current_dir: &PathBuf,
) -> Result<()> {
    if let Some(conn_str) = matches.get_one::<String>("connection-string") {
        config.connection_string = conn_str.clone();
    }

    if let Some(entity_path) = matches.get_one::<String>("entity-path") {
        config.entity_path = entity_path.clone();
    }

    if let Some(base_folder) = matches.get_one::<String>("base-data-folder") {
        let path = PathBuf::from(base_folder);
        config.inbound_config.base_data_folder = if path.is_absolute() {
            path.to_string_lossy().to_string()
        } else {
            current_dir.join(path).to_string_lossy().to_string()
        };
    }

    if let Some(consumer_group) = matches.get_one::<String>("consumer-group") {
        config.inbound_config.consumer_group = consumer_group.clone();
    }

    if let Some(partition_id) = matches.get_one::<i32>("partition-id") {
        config.inbound_config.partition_id = *partition_id;
    }

    if let Some(received_msg_path) = matches.get_one::<String>("received-msg-path") {
        config.inbound_config.received_msg_path = received_msg_path.clone();
    }

    if let Some(db_path) = matches.get_one::<String>("database-path") {
        config.inbound_config.database_path = db_path.clone();
    }

    if matches.get_flag("read-to-file") {
        config.inbound_config.read_to_file = true;
    }

    if matches.get_flag("ignore-checkpoint") {
        config.inbound_config.ignore_checkpoint = true;
    }

    if matches.get_flag("dump-content-only") {
        config.inbound_config.dump_content_only = true;
    }

    if let Some(feedback_interval) = matches.get_one::<u64>("feedback-interval") {
        config.inbound_config.feedback_interval = *feedback_interval;
    }

    if matches.get_flag("verbose") {
        config.verbose = true;
    }

    if let Some(filters) = matches.get_many::<String>("dump-filter") {
        config.inbound_config.dump_filter = Some(filters.cloned().collect());
    }

    if !PathBuf::from(&config.inbound_config.base_data_folder).is_absolute() {
        config.inbound_config.base_data_folder = current_dir
            .join(&config.inbound_config.base_data_folder)
            .to_string_lossy()
            .to_string();
    }

    Ok(())
}

/// Validates required EventHub configuration parameters.
///
/// Ensures connection string, entity path, and consumer group are provided.
///
/// # Errors
/// Returns error with a descriptive message if required parameters are missing.
pub fn validate_config(config: &EventHubConfig) -> Result<()> {
    if config.connection_string.is_empty() {
        return Err(anyhow!("EventHub connection string is required. Use --connection-string or provide it in config file."));
    }

    if config.entity_path.is_empty() {
        return Err(anyhow!(
            "EventHub entity path is required. Use --entity-path or provide it in config file."
        ));
    }

    if config.inbound_config.consumer_group.is_empty() {
        return Err(anyhow!("Consumer group cannot be empty."));
    }

    Ok(())
}