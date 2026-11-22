//! Real search cursors and pages for the Export model's tests: a temp-folder archive seeded
//! through the public store API.

use crate::read::test_support::captured;
use shared_eventhub_core::connection::EventHubIdentity;
use shared_eventhub_core::engine::{Checkpoint, MessageSink};
use shared_eventhub_core::models::CapturedMessage;
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::search::{SearchCursor, SearchPage, SearchQuery};
use shared_eventhub_persistence::store::MessageStore;
use std::num::NonZeroUsize;
use tempfile::TempDir;

/// `count` messages on partition 0 with sequence numbers from 0.
pub fn messages(count: usize) -> Vec<CapturedMessage> {
    (0..count)
        .map(|sequence| captured("0", sequence as i64, &format!("body {sequence}")))
        .collect()
}

/// An archive holding `messages(count)`; the folder lives as long as the value.
pub struct SeededArchive {
    pub store: MessageStore,
    _dir: TempDir,
}

impl SeededArchive {
    pub fn new(count: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = MessageStore::open(&DataPaths::new(dir.path().join("data"))).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            store
                .bind(&EventHubIdentity {
                    fully_qualified_namespace: "orders.servicebus.windows.net".to_owned(),
                    event_hub_name: "orders".to_owned(),
                    created_on: None,
                })
                .await
                .unwrap();
            for message in messages(count) {
                let checkpoint = Checkpoint {
                    partition_id: message.key.partition_id.clone(),
                    sequence_number: message.key.sequence_number,
                    offset: message.offset.clone(),
                    enqueued_time: message.enqueued_time,
                    updated_at: message.processed_at,
                };
                store.store(&message, &checkpoint, true).await.unwrap();
            }
        });
        Self { store, _dir: dir }
    }

    /// A page of at most `limit` messages after `after`.
    pub fn page(&self, limit: usize, after: Option<SearchCursor>) -> SearchPage {
        self.store
            .search(&SearchQuery {
                limit: NonZeroUsize::new(limit),
                after,
                ..SearchQuery::default()
            })
            .unwrap()
    }
}

/// A cursor from a real archive; the model treats it as opaque.
pub fn cursor() -> SearchCursor {
    SeededArchive::new(2).page(1, None).next.unwrap()
}

/// A page of `items` followed by `next`.
pub fn page(items: Vec<CapturedMessage>, next: Option<SearchCursor>) -> SearchPage {
    SearchPage {
        items,
        next,
        scanned: 0,
    }
}
