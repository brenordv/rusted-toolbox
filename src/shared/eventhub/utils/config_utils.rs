use crate::shared::eventhub::eventhub_models::{EventHubConfig, InboundConfig};
use crate::shared::system::load_json_file_to_object::load_json_file_to_object;
use clap::ArgMatches;
use std::path::PathBuf;
use tracing::info;

pub async fn get_base_config_object(
    matches: &ArgMatches,
    current_dir: &PathBuf,
) -> anyhow::Result<EventHubConfig, anyhow::Error> {
    if let Some(config_path) = matches.get_one::<PathBuf>("config") {
        let config_path = if config_path.is_absolute() {
            config_path.clone()
        } else {
            current_dir.join(config_path)
        };

        info!("Loading configuration from: {:?}", config_path);
        let config = load_json_file_to_object::<EventHubConfig>(&config_path).await?;
        Ok(config)
    } else {
        info!("No default configuration file found, creating default config");

        // Create default config
        Ok(EventHubConfig {
            connection_string: String::new(),
            entity_path: String::new(),
            verbose: false,
            inbound_config: InboundConfig::default(),
            export_config: Default::default(),
        })
    }
}
