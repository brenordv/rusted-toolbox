//! No diagnostic event may carry a message body or a secret. This is the only
//! test in its binary, so it can install a global subscriber that sees every
//! event from every thread, the archive writer's included.

use chrono::{TimeZone, Utc};
use eh_core::connection::{DEFAULT_CONSUMER_GROUP, EventHubIdentity};
use eh_core::engine::{Checkpoint, MessageSink};
use eh_core::models::{BodyKind, CapturedMessage, MessageKey};
use eh_persistence::config::{
    AuthMethodConfig, SensitiveValues, StoredSecret, TargetConfig, ToolConfig, resolve_target,
    save_config,
};
use eh_persistence::export::{ExportFormat, ExportOptions};
use eh_persistence::paths::DataPaths;
use eh_persistence::search::SearchQuery;
use eh_persistence::secrets::SecretsStore;
use eh_persistence::store::MessageStore;
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

const SENTINEL_BODY: &str = "sentinel-body-7f3a";
const SENTINEL_SECRET: &str = "Endpoint=sb://x/;SharedAccessKey=sentinel-secret-9c1d";

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

fn sentinel_message() -> CapturedMessage {
    let at = Utc.with_ymd_and_hms(2026, 7, 2, 12, 0, 0).unwrap();
    CapturedMessage {
        key: MessageKey {
            partition_id: "0".to_owned(),
            sequence_number: 1,
        },
        offset: Some("offset-1".to_owned()),
        partition_key: None,
        enqueued_time: Some(at),
        processed_at: at,
        message_id: None,
        correlation_id: None,
        content_type: None,
        properties: BTreeMap::new(),
        body: SENTINEL_BODY.to_owned(),
        body_kind: BodyKind::Utf8,
    }
}

#[tokio::test]
async fn no_event_carries_a_message_body_or_a_secret() {
    let buffer = SharedBuffer::default();
    let writer = buffer.clone();
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish(),
    )
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(root.path().join("data"));
    let captured = sentinel_message();

    let store = MessageStore::open(&paths).unwrap();
    store
        .bind(&EventHubIdentity {
            fully_qualified_namespace: "orders.servicebus.windows.net".to_owned(),
            event_hub_name: "orders".to_owned(),
            created_on: None,
        })
        .await
        .unwrap();
    store
        .store(
            &captured,
            &Checkpoint {
                partition_id: "0".to_owned(),
                sequence_number: 1,
                offset: captured.offset.clone(),
                enqueued_time: captured.enqueued_time,
                updated_at: captured.processed_at,
            },
            true,
        )
        .await
        .unwrap();
    store
        .export(
            &SearchQuery::default(),
            &ExportOptions {
                format: ExportFormat::Json,
                include_metadata: true,
                condense_output: true,
                use_local_time: false,
                export_folder: root.path().join("export"),
                skip_already_exported: true,
            },
            |_| {},
            &AtomicBool::new(false),
        )
        .unwrap();
    let config = ToolConfig::new(TargetConfig {
        namespace: None,
        event_hub: Some("orders".to_owned()),
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        auth: AuthMethodConfig::ConnectionString {
            stored: StoredSecret::LocalDb,
        },
    });
    let secrets = SecretsStore::open(&paths).unwrap();
    save_config(
        root.path(),
        &config,
        &secrets,
        &SensitiveValues {
            connection_string: Some(SENTINEL_SECRET.to_owned().into()),
            client_secret: None,
        },
    )
    .unwrap();
    resolve_target(&config, &secrets).unwrap();
    store.close().await.unwrap();

    let logs = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
    assert!(
        logs.contains("Bound the archive to its event hub"),
        "{logs}"
    );
    assert!(logs.contains("The archive writer stopped"), "{logs}");
    assert!(!logs.contains(SENTINEL_BODY), "{logs}");
    assert!(!logs.contains("sentinel-secret"), "{logs}");
}
