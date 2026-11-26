//! How a captured message is written out: the full details block, the
//! csv/json rows, and the file-safe timestamps used in export paths.

use anyhow::Context;
use chrono::{DateTime, Local, Utc};
use eh_core::models::{BodyKind, CapturedMessage};
use serde::Serialize;
use std::borrow::Cow;
use std::collections::BTreeMap;

/// Timestamp format of the details block and the csv/json `timestamp` field.
pub(crate) const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.9fZ";
const UNAVAILABLE: &str = "[unavailable]";

/// Renders `message` as the details block: broker metadata, the content
/// type, the correlation id, the application properties (one indented
/// `key: value` line each, in key order, or `properties: none`), then the
/// body. A body that is not the exact payload carries a `body note` line
/// saying why. A line break in the content type, the correlation id, or a
/// property's key or value is written escaped (`\n`, `\r`, or `\u{...}` for
/// U+0085, U+2028, and U+2029), so each stays on its own line of the block,
/// and a backslash is written as `\\`.
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
         content type: {content_type}\n\
         correlation id: {correlation_id}\n\
         {properties}\
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
        content_type = one_line_or_unavailable(message.content_type.as_deref()),
        correlation_id = one_line_or_unavailable(message.correlation_id.as_deref()),
        properties = properties_lines(&message.properties),
        body = message.body,
    )
}

/// `value` on one line, or `[unavailable]` when there is none.
fn one_line_or_unavailable(value: Option<&str>) -> Cow<'_, str> {
    value.map_or(Cow::Borrowed(UNAVAILABLE), one_line)
}

/// `text` on one line: a line feed is written as `\n`, a carriage return as
/// `\r`, and the line separators U+0085, U+2028, and U+2029 as `\u{85}`,
/// `\u{2028}`, and `\u{2029}`. A backslash is written as `\\`, so a typed
/// backslash followed by `n` reads differently from a line feed.
fn one_line(text: &str) -> Cow<'_, str> {
    if !text.contains(['\\', '\n', '\r', '\u{85}', '\u{2028}', '\u{2029}']) {
        return Cow::Borrowed(text);
    }
    let mut line = String::with_capacity(text.len() + 1);
    for character in text.chars() {
        match character {
            '\\' => line.push_str("\\\\"),
            '\n' => line.push_str("\\n"),
            '\r' => line.push_str("\\r"),
            '\u{85}' => line.push_str("\\u{85}"),
            '\u{2028}' => line.push_str("\\u{2028}"),
            '\u{2029}' => line.push_str("\\u{2029}"),
            other => line.push(other),
        }
    }
    Cow::Owned(line)
}

/// The details block's properties lines: `properties:` and one indented
/// `key: value` line per property, or `properties: none`.
fn properties_lines(properties: &BTreeMap<String, String>) -> String {
    if properties.is_empty() {
        return "properties: none\n".to_owned();
    }
    let mut lines = "properties:\n".to_owned();
    for (key, value) in properties {
        lines.push_str(&format!("  {}: {}\n", one_line(key), one_line(value)));
    }
    lines
}

/// The csv/json row of a message with metadata. Field order is the csv
/// column order and the json key order; in json an absent content type or
/// correlation id is `null` and `properties` is an object.
#[derive(Serialize)]
pub(crate) struct MetadataRow<'a> {
    pub(crate) entity_path: &'a str,
    pub(crate) partition_id: &'a str,
    pub(crate) event_id: String,
    pub(crate) timestamp: String,
    pub(crate) content_type: Option<&'a str>,
    pub(crate) correlation_id: Option<&'a str>,
    pub(crate) properties: &'a BTreeMap<String, String>,
    pub(crate) message_content: &'a str,
}

impl<'a> MetadataRow<'a> {
    pub(crate) fn new(message: &'a CapturedMessage, entity_path: &'a str) -> Self {
        Self {
            entity_path,
            partition_id: &message.key.partition_id,
            event_id: message.key.sequence_number.to_string(),
            timestamp: message.processed_at.format(TIMESTAMP_FORMAT).to_string(),
            content_type: message.content_type.as_deref(),
            correlation_id: message.correlation_id.as_deref(),
            properties: &message.properties,
            message_content: &message.body,
        }
    }

    /// The properties as the text of a JSON object, the csv `properties`
    /// cell.
    ///
    /// # Errors
    /// Fails when the properties cannot be serialized.
    pub(crate) fn properties_cell(&self) -> anyhow::Result<String> {
        serde_json::to_string(self.properties).context("Failed to serialize the properties")
    }

    /// The csv record, with `properties_cell` in the `properties` column. An
    /// absent content type or correlation id is an empty cell.
    pub(crate) fn fields<'b>(&'b self, properties_cell: &'b str) -> [&'b str; 8] {
        [
            self.entity_path,
            self.partition_id,
            &self.event_id,
            &self.timestamp,
            self.content_type.unwrap_or_default(),
            self.correlation_id.unwrap_or_default(),
            properties_cell,
            self.message_content,
        ]
    }
}

pub(crate) const METADATA_HEADER: [&str; 8] = [
    "entity_path",
    "partition_id",
    "event_id",
    "timestamp",
    "content_type",
    "correlation_id",
    "properties",
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
    fn the_details_block_lists_the_metadata_then_the_body() {
        let mut rendered = message("3", 42, "{\"id\":1}");
        rendered.partition_key = Some("customer-7".to_owned());
        rendered.content_type = Some("application/json".to_owned());
        rendered.correlation_id = Some("run-17".to_owned());
        rendered.properties = BTreeMap::from([
            ("source".to_owned(), "checkout".to_owned()),
            ("attempt".to_owned(), "2".to_owned()),
        ]);

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
             content type: application/json\n\
             correlation id: run-17\n\
             properties:\n\
             \x20 attempt: 2\n\
             \x20 source: checkout\n\
             \n\
             ---| MESSAGE BODY |----------------------------------------------------------\n\
             {\"id\":1}\n\
             ---|          EOF |----------------------------------------------------------\n"
        );
    }

    #[test]
    fn a_message_without_event_metadata_says_so_in_the_details_block() {
        let block = format_full_message(&message("0", 1, "x"));

        assert!(
            block.contains(
                "content type: [unavailable]\n\
                 correlation id: [unavailable]\n\
                 properties: none\n\
                 \n\
                 ---| MESSAGE BODY |"
            ),
            "{block}"
        );
    }

    #[test]
    fn line_breaks_in_event_metadata_stay_on_their_own_line_of_the_block() {
        let mut rendered = message("0", 1, "x");
        rendered.content_type = Some("text/plain\nevent offset: forged".to_owned());
        rendered.correlation_id = Some("run\r\nid: forged".to_owned());
        rendered.properties = BTreeMap::from([
            (
                "note\npartition id".to_owned(),
                "first\r\nevent sequence number: 999".to_owned(),
            ),
            (
                "separators".to_owned(),
                "next\u{85}line\u{2028}paragraph\u{2029}end".to_owned(),
            ),
            ("typed".to_owned(), "a\\nb".to_owned()),
        ]);

        let block = format_full_message(&rendered);

        assert!(
            block.contains(
                "content type: text/plain\\nevent offset: forged\n\
                 correlation id: run\\r\\nid: forged\n\
                 properties:\n\
                 \x20 note\\npartition id: first\\r\\nevent sequence number: 999\n\
                 \x20 separators: next\\u{85}line\\u{2028}paragraph\\u{2029}end\n\
                 \x20 typed: a\\\\nb\n\
                 \n"
            ),
            "{block}"
        );
        assert_eq!(block.matches("\nevent sequence number: ").count(), 1);
        assert!(
            !block.contains(['\u{85}', '\u{2028}', '\u{2029}']),
            "{block}"
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
        let properties = row.properties_cell().unwrap();

        assert_eq!(
            row.fields(&properties),
            [
                "orders",
                "2",
                "9",
                "2026-07-02T12:30:09.000000000Z",
                "",
                "",
                "{}",
                "payload"
            ]
        );
    }

    #[test]
    fn a_metadata_row_carries_the_event_metadata_before_the_body() {
        let mut source = message("2", 9, "payload");
        source.content_type = Some("application/json".to_owned());
        source.correlation_id = Some("run-17".to_owned());
        source.properties = BTreeMap::from([("source".to_owned(), "checkout".to_owned())]);

        let row = MetadataRow::new(&source, "orders");
        let properties = row.properties_cell().unwrap();

        assert_eq!(
            row.fields(&properties)[4..],
            [
                "application/json",
                "run-17",
                "{\"source\":\"checkout\"}",
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
