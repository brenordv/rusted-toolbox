//! Messages for the Read model's tests.

use chrono::Utc;
use eh_core::models::{BodyKind, CapturedMessage, MessageKey};
use std::collections::BTreeMap;
use std::sync::Arc;

/// A captured message on `partition` at `sequence` with `body`.
pub fn captured(partition: &str, sequence: i64, body: &str) -> CapturedMessage {
    captured_with_ids(partition, sequence, body, None, None)
}

pub fn captured_with_ids(
    partition: &str,
    sequence: i64,
    body: &str,
    message_id: Option<&str>,
    correlation_id: Option<&str>,
) -> CapturedMessage {
    let now = Utc::now();
    CapturedMessage {
        key: MessageKey {
            partition_id: partition.to_owned(),
            sequence_number: sequence,
        },
        offset: Some((sequence * 10).to_string()),
        partition_key: None,
        enqueued_time: Some(now),
        processed_at: now,
        message_id: message_id.map(str::to_owned),
        correlation_id: correlation_id.map(str::to_owned),
        content_type: None,
        properties: BTreeMap::new(),
        body: body.to_owned(),
        body_kind: BodyKind::Utf8,
    }
}

/// [`captured`], shared as the list holds it.
pub fn message(partition: &str, sequence: i64, body: &str) -> Arc<CapturedMessage> {
    Arc::new(captured(partition, sequence, body))
}

pub fn message_with_ids(
    partition: &str,
    sequence: i64,
    body: &str,
    message_id: Option<&str>,
    correlation_id: Option<&str>,
) -> Arc<CapturedMessage> {
    Arc::new(captured_with_ids(
        partition,
        sequence,
        body,
        message_id,
        correlation_id,
    ))
}
