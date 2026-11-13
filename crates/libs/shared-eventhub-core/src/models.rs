//! The captured-message model and its conversion from Event Hubs SDK events.

use azure_messaging_eventhubs::models::{AmqpSimpleValue, MessageId, ReceivedEventData};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Identity of a captured event: a sequence number is unique within its
/// partition, so the pair is unique within an event hub.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MessageKey {
    pub partition_id: String,
    pub sequence_number: i64,
}

/// How [`CapturedMessage::body`] was obtained from the event payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodyKind {
    /// The payload was valid UTF-8 and is reproduced exactly.
    Utf8,
    /// The payload was not valid UTF-8; invalid sequences were replaced with
    /// U+FFFD, so the body is a lossy rendering of the bytes.
    LossyUtf8,
    /// The event had no single binary data section (an AMQP value or
    /// sequence body, several data sections, or none), so there is no
    /// payload to render and the body is empty.
    Missing,
}

/// One event as captured from a partition, with every field rendered to
/// plain, serializable values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CapturedMessage {
    pub key: MessageKey,
    /// Broker offset; an opaque string, kept for display and debugging.
    pub offset: Option<String>,
    pub partition_key: Option<String>,
    /// Broker-assigned enqueue time; `None` when the event did not carry one
    /// or it was outside the representable range.
    pub enqueued_time: Option<DateTime<Utc>>,
    /// Local time at which the event was captured.
    pub processed_at: DateTime<Utc>,
    pub message_id: Option<String>,
    pub correlation_id: Option<String>,
    pub content_type: Option<String>,
    /// Application properties with every value rendered as a string.
    pub properties: BTreeMap<String, String>,
    pub body: String,
    pub body_kind: BodyKind,
}

/// Converts an SDK event into a [`CapturedMessage`].
///
/// Returns `None` when the event carries no sequence number: without one the
/// event has no [`MessageKey`], and inventing a key would corrupt dedup and
/// checkpointing.
pub(crate) fn captured_from(
    partition_id: &str,
    event: &ReceivedEventData,
    processed_at: DateTime<Utc>,
) -> Option<CapturedMessage> {
    let sequence_number = event.sequence_number()?;
    let data = event.event_data();
    let (body, body_kind) = render_body(data.body());

    Some(CapturedMessage {
        key: MessageKey {
            partition_id: partition_id.to_owned(),
            sequence_number,
        },
        offset: event.offset().clone(),
        partition_key: event.partition_key().clone(),
        enqueued_time: event.enqueued_time().and_then(system_time_to_utc),
        processed_at,
        message_id: data.message_id().map(render_message_id),
        correlation_id: data.correlation_id().map(render_message_id),
        content_type: data.content_type().map(str::to_owned),
        properties: data
            .properties()
            .map(|properties| {
                properties
                    .iter()
                    .map(|(name, value)| (name.clone(), render_property_value(value)))
                    .collect()
            })
            .unwrap_or_default(),
        body,
        body_kind,
    })
}

/// Converts a [`SystemTime`] to UTC at millisecond precision, returning
/// `None` when it falls outside the range `DateTime<Utc>` can hold.
///
/// chrono's `From<SystemTime>` panics on such values, and event timestamps
/// come from remote producers, so every SDK timestamp goes through here.
pub fn system_time_to_utc(time: SystemTime) -> Option<DateTime<Utc>> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after_epoch) => utc_from_epoch_offset(after_epoch, false),
        Err(before_epoch) => utc_from_epoch_offset(before_epoch.duration(), true),
    }
}

/// The UTC instant `offset` after (or before) the Unix epoch, or `None` when
/// it is outside the range `DateTime<Utc>` can hold.
fn utc_from_epoch_offset(offset: std::time::Duration, before_epoch: bool) -> Option<DateTime<Utc>> {
    let millis = i64::try_from(offset.as_millis()).ok()?;

    DateTime::from_timestamp_millis(if before_epoch { -millis } else { millis })
}

fn render_body(payload: Option<&[u8]>) -> (String, BodyKind) {
    match payload {
        None => (String::new(), BodyKind::Missing),
        Some(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => (text.to_owned(), BodyKind::Utf8),
            Err(_) => (
                String::from_utf8_lossy(bytes).into_owned(),
                BodyKind::LossyUtf8,
            ),
        },
    }
}

fn render_message_id(id: &MessageId) -> String {
    match id {
        MessageId::String(value) => value.clone(),
        MessageId::Uuid(value) => value.to_string(),
        MessageId::Ulong(value) => value.to_string(),
        MessageId::Binary(bytes) => to_lower_hex(bytes),
    }
}

fn render_property_value(value: &AmqpSimpleValue) -> String {
    match value {
        AmqpSimpleValue::Null => "null".to_owned(),
        AmqpSimpleValue::Boolean(value) => value.to_string(),
        AmqpSimpleValue::UByte(value) => value.to_string(),
        AmqpSimpleValue::Byte(value) => value.to_string(),
        AmqpSimpleValue::Char(value) => value.to_string(),
        AmqpSimpleValue::UShort(value) => value.to_string(),
        AmqpSimpleValue::Short(value) => value.to_string(),
        AmqpSimpleValue::UInt(value) => value.to_string(),
        AmqpSimpleValue::Int(value) => value.to_string(),
        AmqpSimpleValue::ULong(value) => value.to_string(),
        AmqpSimpleValue::Long(value) => value.to_string(),
        AmqpSimpleValue::Float(value) => value.to_string(),
        AmqpSimpleValue::Double(value) => value.to_string(),
        AmqpSimpleValue::TimeStamp(timestamp) => timestamp
            .0
            .and_then(system_time_to_utc)
            .map(|time| time.to_rfc3339_opts(SecondsFormat::Millis, true))
            .unwrap_or_default(),
        AmqpSimpleValue::Uuid(value) => value.to_string(),
        AmqpSimpleValue::String(value) => value.clone(),
        AmqpSimpleValue::Symbol(symbol) => symbol.0.clone(),
        AmqpSimpleValue::Binary(bytes) => to_lower_hex(bytes),
        AmqpSimpleValue::Decimal128(_)
        | AmqpSimpleValue::Decimal64(_)
        | AmqpSimpleValue::Decimal32(_)
        | AmqpSimpleValue::Described(_) => format!("{value:?}"),
    }
}

fn to_lower_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";

    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push(char::from(DIGITS[usize::from(byte >> 4)]));
        hex.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    hex
}

#[cfg(test)]
pub(crate) mod test_events {
    //! Builders for realistic SDK events, shared by the conversion and engine
    //! tests. Annotation keys and value types mirror what the broker sends.

    use azure_core_amqp::message::{AmqpAnnotationKey, AmqpAnnotations, AmqpMessageBody};
    use azure_core_amqp::{AmqpMessage, AmqpSymbol, AmqpTimestamp, AmqpValue};
    use azure_messaging_eventhubs::models::ReceivedEventData;
    use chrono::{DateTime, Utc};
    use std::time::SystemTime;

    pub(crate) fn annotations(
        sequence_number: Option<i64>,
        enqueued_time: Option<DateTime<Utc>>,
    ) -> AmqpAnnotations {
        let mut annotations = AmqpAnnotations::new();
        if let Some(sequence_number) = sequence_number {
            annotations.insert(
                AmqpAnnotationKey::Symbol(AmqpSymbol("x-opt-sequence-number".to_owned())),
                AmqpValue::Long(sequence_number),
            );
            annotations.insert(
                AmqpAnnotationKey::Symbol(AmqpSymbol("x-opt-offset".to_owned())),
                AmqpValue::String(format!("offset-{sequence_number}")),
            );
        }
        if let Some(enqueued_time) = enqueued_time {
            annotations.insert(
                AmqpAnnotationKey::Symbol(AmqpSymbol("x-opt-enqueued-time".to_owned())),
                AmqpValue::TimeStamp(AmqpTimestamp(Some(SystemTime::from(enqueued_time)))),
            );
        }
        annotations
    }

    pub(crate) fn event(
        sequence_number: i64,
        enqueued_time: DateTime<Utc>,
        body: &str,
    ) -> ReceivedEventData {
        let message = AmqpMessage::builder()
            .with_body(AmqpMessageBody::Binary(vec![body.as_bytes().to_vec()]))
            .with_message_annotations(annotations(Some(sequence_number), Some(enqueued_time)))
            .build();

        ReceivedEventData::from(message)
    }
}

#[cfg(test)]
mod tests {
    use super::test_events::{annotations, event};
    use super::*;
    use azure_core_amqp::message::{AmqpMessageBody, AmqpMessageId, AmqpMessageProperties};
    use azure_core_amqp::{AmqpMessage, AmqpSymbol, AmqpTimestamp, AmqpValue};
    use chrono::{Duration, TimeZone};
    use rstest::rstest;

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp_millis(Utc::now().timestamp_millis()).unwrap()
    }

    fn message_with_body(body: AmqpMessageBody) -> ReceivedEventData {
        let message = AmqpMessage::builder()
            .with_body(body)
            .with_message_annotations(annotations(Some(7), Some(now())))
            .build();
        ReceivedEventData::from(message)
    }

    #[test]
    fn captured_from_maps_broker_metadata_and_utf8_body() {
        let enqueued = now() - Duration::minutes(3);
        let processed = now();
        let received = event(42, enqueued, "hello");

        let captured = captured_from("3", &received, processed).unwrap();

        assert_eq!(
            captured.key,
            MessageKey {
                partition_id: "3".to_owned(),
                sequence_number: 42
            }
        );
        assert_eq!(captured.offset.as_deref(), Some("offset-42"));
        assert_eq!(captured.enqueued_time, Some(enqueued));
        assert_eq!(captured.processed_at, processed);
        assert_eq!(captured.body, "hello");
        assert_eq!(captured.body_kind, BodyKind::Utf8);
    }

    #[test]
    fn captured_from_renders_invalid_utf8_lossily() {
        let received = message_with_body(AmqpMessageBody::Binary(vec![vec![b'o', b'k', 0xff]]));

        let captured = captured_from("0", &received, now()).unwrap();

        assert_eq!(captured.body, "ok\u{fffd}");
        assert_eq!(captured.body_kind, BodyKind::LossyUtf8);
    }

    #[rstest]
    #[case::value_section(AmqpMessageBody::Value(AmqpValue::String("text".to_owned())))]
    #[case::two_data_sections(AmqpMessageBody::Binary(vec![b"a".to_vec(), b"b".to_vec()]))]
    #[case::empty(AmqpMessageBody::Empty)]
    fn captured_from_marks_bodies_without_one_data_section_missing(#[case] body: AmqpMessageBody) {
        let received = message_with_body(body);

        let captured = captured_from("0", &received, now()).unwrap();

        assert_eq!(captured.body, "");
        assert_eq!(captured.body_kind, BodyKind::Missing);
    }

    #[test]
    fn captured_from_returns_none_without_a_sequence_number() {
        let message = AmqpMessage::builder()
            .with_body(AmqpMessageBody::Binary(vec![b"x".to_vec()]))
            .with_message_annotations(annotations(None, Some(now())))
            .build();

        let captured = captured_from("0", &ReceivedEventData::from(message), now());

        assert!(captured.is_none());
    }

    #[rstest]
    #[case::string(AmqpMessageId::String("order-1".to_owned()), "order-1")]
    #[case::ulong(AmqpMessageId::Ulong(1234), "1234")]
    #[case::binary(AmqpMessageId::Binary(vec![0x0a, 0xff]), "0aff")]
    #[case::uuid(
        AmqpMessageId::Uuid("6f1b5f2a-3c4d-4e5f-8a9b-0c1d2e3f4a5b".parse().unwrap()),
        "6f1b5f2a-3c4d-4e5f-8a9b-0c1d2e3f4a5b"
    )]
    fn captured_from_renders_every_message_id_variant(
        #[case] id: AmqpMessageId,
        #[case] expected: &str,
    ) {
        let properties = AmqpMessageProperties {
            message_id: Some(id.clone()),
            correlation_id: Some(id),
            content_type: Some(AmqpSymbol("application/json".to_owned())),
            ..Default::default()
        };
        let message = AmqpMessage::builder()
            .with_body(AmqpMessageBody::Binary(vec![b"{}".to_vec()]))
            .with_properties(properties)
            .with_message_annotations(annotations(Some(1), Some(now())))
            .build();

        let captured = captured_from("0", &ReceivedEventData::from(message), now()).unwrap();

        assert_eq!(captured.message_id.as_deref(), Some(expected));
        assert_eq!(captured.correlation_id.as_deref(), Some(expected));
        assert_eq!(captured.content_type.as_deref(), Some("application/json"));
    }

    #[test]
    fn captured_from_renders_property_values_as_strings() {
        let stamp = Utc.with_ymd_and_hms(2026, 7, 2, 12, 34, 56).unwrap();
        let message = AmqpMessage::builder()
            .with_body(AmqpMessageBody::Binary(vec![b"x".to_vec()]))
            .with_message_annotations(annotations(Some(1), Some(now())))
            .add_application_property("text".to_owned(), "plain")
            .add_application_property("count".to_owned(), 17_i32)
            .add_application_property("flag".to_owned(), true)
            .add_application_property("ratio".to_owned(), 0.5_f64)
            .add_application_property("blob".to_owned(), vec![0xde_u8, 0xad])
            .add_application_property("symbol".to_owned(), AmqpSymbol("sym".to_owned()))
            .add_application_property(
                "stamp".to_owned(),
                AmqpTimestamp(Some(SystemTime::from(stamp))),
            )
            .add_application_property("nothing".to_owned(), AmqpSimpleValue::Null)
            .build();

        let captured = captured_from("0", &ReceivedEventData::from(message), now()).unwrap();

        let expected: BTreeMap<String, String> = [
            ("text", "plain"),
            ("count", "17"),
            ("flag", "true"),
            ("ratio", "0.5"),
            ("blob", "dead"),
            ("symbol", "sym"),
            ("stamp", "2026-07-02T12:34:56.000Z"),
            ("nothing", "null"),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect();
        assert_eq!(captured.properties, expected);
    }

    #[test]
    fn render_property_value_leaves_an_unrepresentable_timestamp_empty() {
        let value = AmqpSimpleValue::TimeStamp(AmqpTimestamp(None));

        assert_eq!(render_property_value(&value), "");
    }

    #[test]
    fn system_time_to_utc_round_trips_at_millisecond_precision() {
        let time = now();

        assert_eq!(system_time_to_utc(SystemTime::from(time)), Some(time));
    }

    #[test]
    fn system_time_to_utc_handles_times_before_the_epoch() {
        let before_epoch = UNIX_EPOCH - std::time::Duration::from_millis(1_500);

        assert_eq!(
            system_time_to_utc(before_epoch),
            DateTime::from_timestamp_millis(-1_500)
        );
    }

    #[rstest]
    #[case::beyond_chrono_after(std::time::Duration::from_secs(10_000_000_000_000), false)]
    #[case::beyond_chrono_before(std::time::Duration::from_secs(10_000_000_000_000), true)]
    #[case::beyond_i64_millis(std::time::Duration::from_secs(u64::MAX), false)]
    fn utc_from_epoch_offset_returns_none_outside_the_chrono_range(
        #[case] offset: std::time::Duration,
        #[case] before_epoch: bool,
    ) {
        assert_eq!(utc_from_epoch_offset(offset, before_epoch), None);
    }
}
