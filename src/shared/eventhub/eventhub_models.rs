use crate::shared::eventhub::eventhub_traits::{
    default_base_data_folder, default_consumer_group, default_database_path,
    default_export_base_data_folder, default_export_folder, default_export_format,
    default_feedback_interval, default_include_metadata, default_partition_id,
    default_received_msg_path,
};
use anyhow::{anyhow, Error};
use azeventhubs::consumer::{EventHubConsumerClient, EventHubConsumerClientOptions};
use azeventhubs::BasicRetryPolicy;
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventHubConfig {
    #[serde(rename = "eventhubConnString")]
    pub connection_string: String,

    #[serde(rename = "entityPath")]
    pub entity_path: String,

    #[serde(default)]
    pub verbose: bool,

    #[serde(default)]
    pub inbound_config: InboundConfig,

    #[serde(default)]
    pub export_config: ExportConfig,
}

impl EventHubConfig {
    pub async fn create_consumer_client(
        &self,
    ) -> Result<EventHubConsumerClient<BasicRetryPolicy>, Error> {
        match EventHubConsumerClient::new_from_connection_string(
            &self.inbound_config.consumer_group,
            &self.connection_string,
            self.entity_path.clone(),
            EventHubConsumerClientOptions::default(),
        )
        .await
        {
            Ok(client) => Ok(client),
            Err(err) => Err(err.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundConfig {
    #[serde(default = "default_consumer_group")]
    pub consumer_group: String,

    #[serde(default = "default_partition_id")]
    pub partition_id: i32,

    #[serde(default = "default_received_msg_path")]
    pub received_msg_path: String,

    #[serde(default = "default_database_path")]
    pub database_path: String,

    #[serde(default = "default_base_data_folder")]
    pub base_data_folder: String,

    #[serde(default = "default_feedback_interval")]
    pub feedback_interval: u64,

    #[serde(default)]
    pub read_to_file: bool,

    #[serde(default)]
    pub ignore_checkpoint: bool,

    #[serde(default)]
    pub dump_content_only: bool,

    #[serde(default)]
    pub dump_filter: Option<Vec<String>>,
}

impl InboundConfig {
    pub fn get_partition_id_label(&self) -> String {
        if self.partition_id == -1 {
            "ALL".to_string()
        } else {
            self.partition_id.to_string()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    #[serde(default = "default_export_base_data_folder")]
    pub base_data_folder: String,

    #[serde(default = "default_export_format")]
    pub export_format: String,

    #[serde(default)]
    pub condense_output: bool,

    #[serde(default = "default_include_metadata")]
    pub include_metadata: bool,

    #[serde(default)]
    pub ignore_checkpoint: bool,

    #[serde(default)]
    pub dump_filter: Option<Vec<String>>,

    #[serde(default = "default_export_folder")]
    pub export_folder: String,

    #[serde(default = "default_feedback_interval")]
    pub feedback_interval: u64,

    #[serde(default)]
    pub use_local_time: bool,

    #[serde(default = "default_database_path")]
    pub database_path: String,
}

impl ExportConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if !["txt", "csv", "json"].contains(&self.export_format.as_str()) {
            return Err(anyhow!(
                "Invalid export_format '{}'. Valid options are: txt, csv, json",
                self.export_format
            ));
        }

        //The value is an u64, so there won't be any negative values.
        if self.feedback_interval == 0 {
            return Err(anyhow!(
                "feedback_interval must be positive, got: {}",
                self.feedback_interval
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventHubCheckpoint {
    pub sequence_number: i64,
    pub offset: String,
    pub partition_id: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    pub id: String,
    pub event_id: String,
    pub partition_key: Option<String>,
    pub partition_id: String,
    pub queued_time: DateTime<Utc>,
    pub event_seq_number: Option<i64>,
    pub event_offset: Option<String>,
    pub suggested_filename: Option<String>,
    pub processed_at: DateTime<Utc>,
    pub msg_data: String,
    pub status: MessageStatus,
}

impl InboundMessage {
    pub fn get_file_safe_timestamp_yyyy_mm_dd(&self, use_local_time: bool) -> (String, String) {
        if use_local_time {
            (
                self.processed_at
                    .with_timezone(&Local)
                    .format("%Y-%m")
                    .to_string(),
                self.processed_at
                    .with_timezone(&Local)
                    .format("%d")
                    .to_string(),
            )
        } else {
            (
                self.processed_at
                    .with_timezone(&Utc)
                    .format("%Y-%m")
                    .to_string(),
                self.processed_at
                    .with_timezone(&Utc)
                    .format("%d")
                    .to_string(),
            )
        }
    }

    pub fn get_file_safe_timestamp_full(&self, use_local_time: bool) -> String {
        if use_local_time {
            self.processed_at
                .with_timezone(&Local)
                .format("%Y-%m-%d-T-%H-%M-%S-%f")
                .to_string()
        } else {
            self.processed_at
                .with_timezone(&Utc)
                .format("%Y-%m-%d-T-%H-%M-%S-%f")
                .to_string()
        }
    }

    pub fn format_full_message_to_string(&self) -> String {
        format!(
            r#"---| DETAILS      |----------------------------------------------------------
id: {}
partition key: {}
added to queue at: {}
partition id: {}
event sequence number: {}
event offset: {}
Message processed at: {}
Filename: {}

---| MESSAGE BODY |----------------------------------------------------------
{}
---|          EOF |----------------------------------------------------------
"#,
            self.event_id,
            self.partition_key.as_deref().unwrap_or("[unavailable]"),
            self.queued_time.format("%Y-%m-%dT%H:%M:%S%.9fZ"),
            self.partition_id,
            self.event_seq_number.unwrap_or(0),
            self.event_offset.as_deref().unwrap_or("[unavailable]"),
            self.processed_at.format("%Y-%m-%dT%H:%M:%S%.9fZ"),
            self.suggested_filename
                .as_deref()
                .unwrap_or("[unavailable]"),
            self.msg_data
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageStatus {
    Read,
    Exported,
}
