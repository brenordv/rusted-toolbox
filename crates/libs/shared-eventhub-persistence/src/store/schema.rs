//! The archive's tables and the encoding of what they store.
//!
//! Stored values are the serde_json form of `shared-eventhub-core`'s
//! `CapturedMessage` and `Checkpoint`, so a change to those types can change
//! the on-disk format. The rule: adding an `Option` field, or a field with
//! `#[serde(default)]`, keeps schema version 1; stored types never use
//! `deny_unknown_fields`, so an older build ignores fields it does not know;
//! a new `BodyKind` variant, a rename, a removal, or a type change needs a new
//! [`SCHEMA_VERSION`]. The golden-bytes tests below fail when the encoding
//! moves.

use anyhow::Context;
use chrono::{DateTime, SecondsFormat, Utc};
use redb::TableDefinition;
use shared_eventhub_core::engine::Checkpoint;
use shared_eventhub_core::models::{CapturedMessage, MessageKey};

/// Archive metadata: the schema version and the bound event hub.
pub(crate) const META: TableDefinition<&str, &str> = TableDefinition::new("meta");
/// Captured messages keyed by (partition id, sequence number).
pub(crate) const MESSAGES: TableDefinition<(&str, i64), &[u8]> = TableDefinition::new("messages");
/// Time index: (enqueued unix ms, partition id, sequence number).
pub(crate) const BY_TIME: TableDefinition<(i64, &str, i64), ()> = TableDefinition::new("by_time");
/// The latest checkpoint per partition.
pub(crate) const CHECKPOINTS: TableDefinition<&str, &[u8]> = TableDefinition::new("checkpoints");
/// When each message was last exported (unix ms).
pub(crate) const EXPORT_LOG: TableDefinition<(&str, i64), i64> = TableDefinition::new("export_log");

pub(crate) const SCHEMA_VERSION_KEY: &str = "schema_version";
pub(crate) const SCHEMA_VERSION: &str = "1";
pub(crate) const NAMESPACE_KEY: &str = "namespace";
pub(crate) const EVENT_HUB_KEY: &str = "event_hub";
/// When the service created the bound event hub, RFC 3339 UTC with milliseconds; absent when
/// the service did not report it or the archive was bound before the key existed.
pub(crate) const CREATED_ON_KEY: &str = "created_on";

/// A creation time as stored under [`CREATED_ON_KEY`].
pub(crate) fn encode_created_on(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// The time-index key of a message without an enqueue time. A real enqueue
/// time can never be this many milliseconds, since it lies outside chrono's
/// range, and it sorts before every real time.
pub(crate) const NO_ENQUEUED_TIME: i64 = i64::MIN;

/// The time-index key of `message`.
pub(crate) fn time_key(message: &CapturedMessage) -> i64 {
    message
        .enqueued_time
        .map_or(NO_ENQUEUED_TIME, |time| time.timestamp_millis())
}

pub(crate) fn encode_message(message: &CapturedMessage) -> anyhow::Result<Vec<u8>> {
    serde_json::to_vec(message).with_context(|| {
        format!(
            "Failed to encode the message at partition {}, sequence {}",
            message.key.partition_id, message.key.sequence_number
        )
    })
}

pub(crate) fn decode_message(
    partition_id: &str,
    sequence_number: i64,
    bytes: &[u8],
) -> anyhow::Result<CapturedMessage> {
    serde_json::from_slice(bytes).with_context(|| {
        format!(
            "The archived message at partition {partition_id}, sequence {sequence_number} could \
             not be decoded"
        )
    })
}

pub(crate) fn encode_checkpoint(checkpoint: &Checkpoint) -> anyhow::Result<Vec<u8>> {
    serde_json::to_vec(checkpoint).with_context(|| {
        format!(
            "Failed to encode the checkpoint of partition {}",
            checkpoint.partition_id
        )
    })
}

pub(crate) fn decode_checkpoint(partition_id: &str, bytes: &[u8]) -> anyhow::Result<Checkpoint> {
    serde_json::from_slice(bytes)
        .with_context(|| format!("The checkpoint of partition {partition_id} could not be decoded"))
}

/// A message key as the borrowed tuple the `messages` table uses.
pub(crate) fn message_table_key(key: &MessageKey) -> (&str, i64) {
    (key.partition_id.as_str(), key.sequence_number)
}

#[cfg(test)]
pub(crate) mod test_messages {
    //! Message and checkpoint builders shared by the crate's tests.

    use chrono::{DateTime, TimeZone, Utc};
    use shared_eventhub_core::engine::Checkpoint;
    use shared_eventhub_core::models::{BodyKind, CapturedMessage, MessageKey};
    use std::collections::BTreeMap;

    /// A fixed instant: the golden encodings and export file names depend on
    /// exact timestamps.
    pub(crate) fn instant(hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 2, hour, minute, second)
            .unwrap()
    }

    pub(crate) fn message(partition_id: &str, sequence_number: i64, body: &str) -> CapturedMessage {
        CapturedMessage {
            key: MessageKey {
                partition_id: partition_id.to_owned(),
                sequence_number,
            },
            offset: Some(format!("offset-{sequence_number}")),
            partition_key: None,
            enqueued_time: Some(instant(12, 0, 0) + chrono::Duration::seconds(sequence_number)),
            processed_at: instant(12, 30, 0) + chrono::Duration::seconds(sequence_number),
            message_id: None,
            correlation_id: None,
            content_type: None,
            properties: BTreeMap::new(),
            body: body.to_owned(),
            body_kind: BodyKind::Utf8,
        }
    }

    pub(crate) fn checkpoint_for(message: &CapturedMessage) -> Checkpoint {
        Checkpoint {
            partition_id: message.key.partition_id.clone(),
            sequence_number: message.key.sequence_number,
            offset: message.offset.clone(),
            enqueued_time: message.enqueued_time,
            updated_at: message.processed_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_messages::{checkpoint_for, instant};
    use super::*;
    use shared_eventhub_core::models::BodyKind;
    use std::collections::BTreeMap;

    fn fully_populated() -> CapturedMessage {
        CapturedMessage {
            key: MessageKey {
                partition_id: "3".to_owned(),
                sequence_number: 42,
            },
            offset: Some("offset-42".to_owned()),
            partition_key: Some("customer-7".to_owned()),
            enqueued_time: Some(instant(12, 0, 0)),
            processed_at: instant(12, 0, 5),
            message_id: Some("message-1".to_owned()),
            correlation_id: Some("correlation-1".to_owned()),
            content_type: Some("application/json".to_owned()),
            properties: BTreeMap::from([("source".to_owned(), "orders".to_owned())]),
            body: "{\"id\":1}".to_owned(),
            body_kind: BodyKind::LossyUtf8,
        }
    }

    #[test]
    fn message_encoding_matches_the_schema_v1_golden_bytes() {
        let encoded = encode_message(&fully_populated()).unwrap();

        assert_eq!(
            String::from_utf8(encoded).unwrap(),
            r#"{"key":{"partition_id":"3","sequence_number":42},"offset":"offset-42","partition_key":"customer-7","enqueued_time":"2026-07-02T12:00:00Z","processed_at":"2026-07-02T12:00:05Z","message_id":"message-1","correlation_id":"correlation-1","content_type":"application/json","properties":{"source":"orders"},"body":"{\"id\":1}","body_kind":"LossyUtf8"}"#
        );
    }

    #[test]
    fn checkpoint_encoding_matches_the_schema_v1_golden_bytes() {
        let encoded = encode_checkpoint(&checkpoint_for(&fully_populated())).unwrap();

        assert_eq!(
            String::from_utf8(encoded).unwrap(),
            r#"{"partition_id":"3","sequence_number":42,"offset":"offset-42","enqueued_time":"2026-07-02T12:00:00Z","updated_at":"2026-07-02T12:00:05Z"}"#
        );
    }

    #[test]
    fn message_round_trips_through_its_encoding() {
        let message = fully_populated();

        let decoded = decode_message("3", 42, &encode_message(&message).unwrap()).unwrap();

        assert_eq!(decoded, message);
    }

    #[test]
    fn a_message_that_does_not_decode_names_its_key() {
        let error = decode_message("3", 42, b"not json").unwrap_err();

        assert!(
            error.to_string().contains("partition 3, sequence 42"),
            "{error:#}"
        );
    }

    #[test]
    fn a_checkpoint_that_does_not_decode_names_its_partition() {
        let error = decode_checkpoint("5", b"{").unwrap_err();

        assert!(error.to_string().contains("partition 5"), "{error:#}");
    }

    #[test]
    fn a_message_without_enqueued_time_sorts_before_every_real_time() {
        let mut message = fully_populated();
        message.enqueued_time = None;

        assert_eq!(time_key(&message), NO_ENQUEUED_TIME);
        assert!(time_key(&message) < time_key(&fully_populated()));
    }
}
