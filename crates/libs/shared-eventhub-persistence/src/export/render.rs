//! How a captured message is written out: the full details block, the
//! csv/json rows, and the file-safe timestamps used in export paths.

use chrono::{DateTime, Local, Utc};
use serde::Serialize;
use shared_eventhub_core::models::{BodyKind, CapturedMessage};

/// Timestamp format of the details block and the csv/json `timestamp` field.
pub(crate) const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.9fZ";
const UNAVAILABLE: &str = "[unavailable]";

/// Renders `message` as the details block: broker metadata, then the body.
/// A body that is not the exact payload carries a `body note` line saying
/// why.
pub fn format_full_message(message: &CapturedMessage) -> String {
    let enqueued = message.enqueued_time.map_or_else(
        || UNAVAILABLE.to_owned(),
        |time| time.format(TIMESTAMP_FORMAT).to_string(),
    );
    let body_note = match message.body_kind {
        BodyKind::Utf8 => String::new(),
        BodyKind::LossyUtf8 => {
            "body note: the payload was not valid UTF-8; invalid bytes were replaced with U+FFFD\n"
                .to_owned()
        }
        BodyKind::Missing => {
            "body note: the event had no single binary data section, so it has no body\n".to_owned()
        }
    };

    format!(
        "---| DETAILS      |----------------------------------------------------------\n\
         id: {sequence}\n\
         partition key: {partition_key}\n\
         added to queue at: {enqueued}\n\
         partition id: {partition}\n\
         event sequence number: {sequence}\n\
         event offset: {offset}\n\
         Message processed at: {processed}\n\
         {body_note}\
         \n\
         ---| MESSAGE BODY |----------------------------------------------------------\n\
         {body}\n\
         ---|          EOF |----------------------------------------------------------\n",
        sequence = message.key.sequence_number,
        partition_key = message.partition_key.as_deref().unwrap_or(UNAVAILABLE),
        partition = message.key.partition_id,
        offset = message.offset.as_deref().unwrap_or(UNAVAILABLE),
        processed = message.processed_at.format(TIMESTAMP_FORMAT),
        body = message.body,
    )
}

/// The csv/json row of a message with metadata. Field order is the csv
/// column order and the json key order.
#[derive(Serialize)]
pub(crate) struct MetadataRow<'a> {
    pub(crate) entity_path: &'a str,
    pub(crate) partition_id: &'a str,
    pub(crate) event_id: String,
    pub(crate) timestamp: String,
    pub(crate) message_content: &'a str,
}

impl<'a> MetadataRow<'a> {
    pub(crate) fn new(message: &'a CapturedMessage, entity_path: &'a str) -> Self {
        Self {
            entity_path,
            partition_id: &message.key.partition_id,
            event_id: message.key.sequence_number.to_string(),
            timestamp: message.processed_at.format(TIMESTAMP_FORMAT).to_string(),
            message_content: &message.body,
        }
    }

    pub(crate) fn fields(&self) -> [&str; 5] {
        [
            self.entity_path,
            self.partition_id,
            &self.event_id,
            &self.timestamp,
            self.message_content,
        ]
    }
}

pub(crate) const METADATA_HEADER: [&str; 5] = [
    "entity_path",
    "partition_id",
    "event_id",
    "timestamp",
    "message_content",
];
pub(crate) const CONTENT_HEADER: [&str; 1] = ["message_content"];

/// The csv/json row of a message without metadata.
#[derive(Serialize)]
pub(crate) struct ContentRow<'a> {
    pub(crate) message_content: &'a str,
}

/// `at` in UTC or local time, formatted with `format`.
fn format_at(at: DateTime<Utc>, use_local_time: bool, format: &str) -> String {
    if use_local_time {
        at.with_timezone(&Local).format(format).to_string()
    } else {
        at.format(format).to_string()
    }
}

/// The `YYYY-MM` folder and `DD` subfolder of an export path.
pub(crate) fn month_and_day(at: DateTime<Utc>, use_local_time: bool) -> (String, String) {
    (
        format_at(at, use_local_time, "%Y-%m"),
        format_at(at, use_local_time, "%d"),
    )
}

/// The timestamp that starts an individual export file's name.
pub(crate) fn file_timestamp(at: DateTime<Utc>, use_local_time: bool) -> String {
    format_at(at, use_local_time, "%Y-%m-%d-T-%H-%M-%S-%f")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::schema::test_messages::{instant, message};
    use rstest::rstest;

    #[test]
    fn the_details_block_keeps_the_old_layout() {
        let mut rendered = message("3", 42, "{\"id\":1}");
        rendered.partition_key = Some("customer-7".to_owned());

        let block = format_full_message(&rendered);

        assert_eq!(
            block,
            "---| DETAILS      |----------------------------------------------------------\n\
             id: 42\n\
             partition key: customer-7\n\
             added to queue at: 2026-07-02T12:00:42.000000000Z\n\
             partition id: 3\n\
             event sequence number: 42\n\
             event offset: offset-42\n\
             Message processed at: 2026-07-02T12:30:42.000000000Z\n\
             \n\
             ---| MESSAGE BODY |----------------------------------------------------------\n\
             {\"id\":1}\n\
             ---|          EOF |----------------------------------------------------------\n"
        );
    }

    #[test]
    fn missing_metadata_renders_as_unavailable() {
        let mut rendered = message("0", 1, "x");
        rendered.enqueued_time = None;
        rendered.offset = None;

        let block = format_full_message(&rendered);

        assert!(block.contains("partition key: [unavailable]\n"), "{block}");
        assert!(
            block.contains("added to queue at: [unavailable]\n"),
            "{block}"
        );
        assert!(block.contains("event offset: [unavailable]\n"), "{block}");
    }

    #[rstest]
    #[case::lossy(BodyKind::LossyUtf8, "body note: the payload was not valid UTF-8")]
    #[case::missing(
        BodyKind::Missing,
        "body note: the event had no single binary data section"
    )]
    fn a_body_that_is_not_the_exact_payload_carries_a_note(
        #[case] kind: BodyKind,
        #[case] note: &str,
    ) {
        let mut rendered = message("0", 1, "");
        rendered.body_kind = kind;

        let block = format_full_message(&rendered);

        assert!(block.contains(note), "{block}");
    }

    #[test]
    fn metadata_rows_use_the_sequence_number_as_event_id() {
        let source = message("2", 9, "payload");

        let row = MetadataRow::new(&source, "orders");

        assert_eq!(
            row.fields(),
            [
                "orders",
                "2",
                "9",
                "2026-07-02T12:30:09.000000000Z",
                "payload"
            ]
        );
    }

    #[test]
    fn utc_paths_use_utc_dates() {
        let at = instant(23, 59, 1);

        assert_eq!(
            month_and_day(at, false),
            ("2026-07".to_owned(), "02".to_owned())
        );
        assert_eq!(file_timestamp(at, false), "2026-07-02-T-23-59-01-000000000");
    }
}
