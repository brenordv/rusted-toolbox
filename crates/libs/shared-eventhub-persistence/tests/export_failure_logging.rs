//! Failed export files are logged one by one up to a cap, then only counted.
//! This is the only test in its binary, so a global subscriber sees every
//! event without other tests' events mixed in.

use chrono::{TimeZone, Utc};
use shared_eventhub_core::connection::EventHubIdentity;
use shared_eventhub_core::engine::{Checkpoint, MessageSink};
use shared_eventhub_core::models::{BodyKind, CapturedMessage, MessageKey};
use shared_eventhub_persistence::export::{ExportFormat, ExportOptions};
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::search::SearchQuery;
use shared_eventhub_persistence::store::MessageStore;
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn message(sequence_number: i64) -> CapturedMessage {
    let at = Utc.with_ymd_and_hms(2026, 7, 2, 12, 0, 0).unwrap()
        + chrono::Duration::seconds(sequence_number);
    CapturedMessage {
        key: MessageKey {
            partition_id: "0".to_owned(),
            sequence_number,
        },
        offset: None,
        partition_key: None,
        enqueued_time: Some(at),
        processed_at: at,
        message_id: None,
        correlation_id: None,
        content_type: None,
        properties: BTreeMap::new(),
        body: "x".to_owned(),
        body_kind: BodyKind::Utf8,
    }
}

#[tokio::test]
async fn the_first_twenty_failures_are_logged_and_the_rest_counted() {
    let buffer = SharedBuffer::default();
    let writer = buffer.clone();
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish(),
    )
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = MessageStore::open(&DataPaths::new(root.path().join("data"))).unwrap();
    store
        .bind(&EventHubIdentity {
            fully_qualified_namespace: "orders.servicebus.windows.net".to_owned(),
            event_hub_name: "orders".to_owned(),
            created_on: None,
        })
        .await
        .unwrap();
    for sequence_number in 1..=25 {
        let stored = message(sequence_number);
        let checkpoint = Checkpoint {
            partition_id: "0".to_owned(),
            sequence_number,
            offset: None,
            enqueued_time: stored.enqueued_time,
            updated_at: stored.processed_at,
        };
        store.store(&stored, &checkpoint, true).await.unwrap();
    }
    let blocker = root.path().join("blocker");
    std::fs::write(&blocker, "a file where the export folder should be").unwrap();

    let outcome = store
        .export(
            &SearchQuery::default(),
            &ExportOptions {
                format: ExportFormat::Txt,
                include_metadata: true,
                condense_output: false,
                use_local_time: false,
                export_folder: blocker,
                skip_already_exported: true,
            },
            |_| {},
            &AtomicBool::new(false),
        )
        .unwrap();

    let logs = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
    assert_eq!(outcome.failed, 25);
    assert_eq!(logs.matches("Failed to write a message file").count(), 20);
    assert_eq!(logs.matches("no longer logged").count(), 1);
}
