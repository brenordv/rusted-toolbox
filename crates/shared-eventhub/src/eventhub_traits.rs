use crate::eventhub_models::{ExportConfig, InboundConfig};

impl Default for InboundConfig {
    fn default() -> Self {
        Self {
            consumer_group: default_consumer_group(),
            partition_id: default_partition_id(),
            received_msg_path: default_received_msg_path(),
            database_path: default_database_path(),
            base_data_folder: default_base_data_folder(),
            feedback_interval: default_feedback_interval(),
            read_to_file: false,
            ignore_checkpoint: false,
            dump_content_only: false,
            dump_filter: None,
        }
    }
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            base_data_folder: default_export_base_data_folder(),
            export_format: default_export_format(),
            condense_output: false,
            include_metadata: default_include_metadata(),
            ignore_checkpoint: false,
            dump_filter: None,
            export_folder: default_export_folder(),
            feedback_interval: default_feedback_interval(),
            use_local_time: false,
            database_path: default_database_path(),
        }
    }
}

#[allow(dead_code)] //Being used as the default value
pub fn default_consumer_group() -> String {
    "$Default".to_string()
}

#[allow(dead_code)] //Being used as the default value
pub fn default_partition_id() -> i32 {
    -1
}

#[allow(dead_code)] //Being used as the default value
pub fn default_base_data_folder() -> String {
    ".eh-read-data".to_string()
}

#[allow(dead_code)] //Being used as the default value
pub fn default_received_msg_path() -> String {
    "inbound".to_string()
}

#[allow(dead_code)] //Being used as the default value
pub fn default_database_path() -> String {
    "db".to_string()
}

#[allow(dead_code)] //Being used as the default value
pub fn default_feedback_interval() -> u64 {
    1
}

#[allow(dead_code)] //Being used as the default value
pub fn default_export_base_data_folder() -> String {
    ".eh-export-data".to_string()
}

#[allow(dead_code)] //Being used as the default value
pub fn default_export_format() -> String {
    "txt".to_string()
}

#[allow(dead_code)] //Being used as the default value
pub fn default_include_metadata() -> bool {
    true
}

#[allow(dead_code)] //Being used as the default value
pub fn default_export_folder() -> String {
    "exports".to_string()
}