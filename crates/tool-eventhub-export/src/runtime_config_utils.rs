use anyhow::{anyhow, Result};
use shared::eventhub::eventhub_models::EventHubConfig;
use std::path::{Path, PathBuf};

/// Applies command-line overrides to the EventHub configuration.
///
/// # Arguments
/// - `config`: Mutable EventHub configuration to update
/// - `matches`: Parsed CLI arguments containing override values
/// - `current_dir`: Current directory for resolving relative paths
///
/// # Returns
/// - `Ok(())`: Configuration updated successfully
/// - `Err`: Path resolution or argument parsing failed
///
/// # Override Categories
/// - Database paths and base folders
/// - Export format and output options
/// - Filtering and checkpoint behavior
pub fn apply_cli_overrides(
    config: &mut EventHubConfig,
    matches: &clap::ArgMatches,
    current_dir: &Path,
) -> Result<()> {
    // Inbound config overrides (for database access)
    if let Some(base_data_folder) = matches.get_one::<String>("base-data-folder") {
        config.inbound_config.base_data_folder = if PathBuf::from(base_data_folder).is_absolute() {
            base_data_folder.clone()
        } else {
            current_dir
                .join(base_data_folder)
                .to_string_lossy()
                .to_string()
        };
    }

    if let Some(database_path) = matches.get_one::<String>("database-path") {
        config.inbound_config.database_path = database_path.clone();
    }

    if let Some(export_database_path) = matches.get_one::<String>("export-database-path") {
        config.export_config.database_path = export_database_path.clone();
    }

    // Export config overrides
    if let Some(export_base_data_folder) = matches.get_one::<String>("export-base-data-folder") {
        config.export_config.base_data_folder =
            if PathBuf::from(export_base_data_folder).is_absolute() {
                export_base_data_folder.clone()
            } else {
                current_dir
                    .join(export_base_data_folder)
                    .to_string_lossy()
                    .to_string()
            };
    }

    if let Some(export_format) = matches.get_one::<String>("export-format") {
        config.export_config.export_format = export_format.clone();
    }

    if matches.get_flag("condense-output") {
        config.export_config.condense_output = true;
    };

    if matches.get_flag("include-metadata") {
        config.export_config.include_metadata = true;
    };

    if matches.get_flag("ignore-checkpoint") {
        config.export_config.ignore_checkpoint = true;
    }

    if let Some(dump_filter) = matches.get_many::<String>("dump-filter") {
        config.export_config.dump_filter = Some(dump_filter.cloned().collect());
    }

    if let Some(export_folder) = matches.get_one::<String>("export-folder") {
        config.export_config.export_folder = export_folder.clone();
    }

    if let Some(feedback_interval) = matches.get_one::<u64>("feedback-interval") {
        config.export_config.feedback_interval = *feedback_interval;
    }

    if matches.get_flag("use-local-time") {
        config.export_config.use_local_time = true;
    }

    Ok(())
}

/// Validates the EventHub export configuration for required fields and valid values.
///
/// # Arguments
/// - `config`: EventHub configuration to validate
///
/// # Returns
/// - `Ok(())`: Configuration is valid
/// - `Err`: Validation failed with specific error message
///
/// # Validation Rules
/// - Export format must be txt, csv, or json
/// - Feedback interval must be positive
/// - Required paths must be specified and exist
/// - Connection string and entity path cannot be empty
pub fn validate_config(config: &EventHubConfig) -> Result<()> {
    // Validate export config
    if !["txt", "csv", "json"].contains(&config.export_config.export_format.as_str()) {
        return Err(anyhow!(
            "Invalid export_format '{}'. Valid options are: txt, csv, json",
            &config.export_config.export_format
        ));
    }

    if config.export_config.feedback_interval == 0 {
        return Err(anyhow!(
            "feedback_interval must be positive, got: {}",
            &config.export_config.feedback_interval
        ));
    }

    // Validate that we have the minimum required config to locate the source database
    if config.inbound_config.base_data_folder.is_empty() {
        return Err(anyhow!(
            "base_data_folder must be specified (for locating the source database)"
        ));
    }

    if config.inbound_config.database_path.is_empty() {
        return Err(anyhow!(
            "database_path must be specified (for locating the source database)"
        ));
    }

    // Validate connection string and entity path are provided
    // Note: These aren't strictly needed for export operations, but they help identify the correct database
    // and are expected to be present in any valid configuration file
    // TODO #1: Change this to avoid needing the full connection string just to export messages.
    if config.connection_string.trim().is_empty() {
        return Err(anyhow!(
            "connection_string (eventhubConnString) cannot be empty"
        ));
    }

    if config.entity_path.trim().is_empty() {
        return Err(anyhow!("entity_path (entityPath) cannot be empty"));
    }

    // Resolve and display actual paths
    let resolved_source_db_path =
        if PathBuf::from(&config.inbound_config.database_path).is_absolute() {
            config.inbound_config.database_path.clone()
        } else {
            PathBuf::from(&config.inbound_config.base_data_folder)
                .join(&config.inbound_config.database_path)
                .to_string_lossy()
                .to_string()
        };

    if !PathBuf::from(&resolved_source_db_path).exists() {
        return Err(anyhow!(
            "Source database path {:?} does not exist",
            resolved_source_db_path
        ));
    }

    Ok(())
}
