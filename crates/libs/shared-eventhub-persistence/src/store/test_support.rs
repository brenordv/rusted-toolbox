//! Archives whose writes fail on demand, for tests of what follows a failed
//! write: a store over a backend that can be switched to fail, and a store
//! whose writer thread dies on its first batch.

use super::writer::test_writers::spawn_panicking;
use super::{MessageStore, StoreInner, open_database};
use crate::paths::DataPaths;
use anyhow::Context;
use redb::backends::InMemoryBackend;
use redb::{BackendError, Builder, StorageBackend};
use shared_eventhub_core::connection::EventHubIdentity;
use shared_eventhub_core::engine::MessageSink;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// An in-memory backend whose writes can be switched to fail.
#[derive(Debug)]
struct FailingBackend {
    inner: InMemoryBackend,
    failing: Arc<AtomicBool>,
}

impl FailingBackend {
    fn check(&self) -> Result<(), std::io::Error> {
        if self.failing.load(Ordering::SeqCst) {
            Err(std::io::Error::other("simulated disk failure"))
        } else {
            Ok(())
        }
    }
}

impl StorageBackend for FailingBackend {
    fn len(&self) -> Result<u64, BackendError> {
        self.inner.len()
    }

    fn read(&self, offset: u64, out: &mut [u8]) -> Result<(), BackendError> {
        self.inner.read(offset, out)
    }

    fn set_len(&self, len: u64) -> Result<(), BackendError> {
        self.check()?;
        self.inner.set_len(len)
    }

    fn sync_data(&self) -> Result<(), BackendError> {
        self.check()?;
        self.inner.sync_data()
    }

    fn write(&self, offset: u64, data: &[u8]) -> Result<(), BackendError> {
        self.check()?;
        self.inner.write(offset, data)
    }
}

/// An in-memory archive named `memory-archive`, bound to the event hub
/// `orders.servicebus.windows.net/orders`, whose writes fail with "simulated
/// disk failure" once the returned flag is set.
///
/// # Errors
/// Fails when the in-memory database cannot be created or bound.
pub async fn failing_store() -> anyhow::Result<(MessageStore, Arc<AtomicBool>)> {
    let failing = Arc::new(AtomicBool::new(false));
    let backend = FailingBackend {
        inner: InMemoryBackend::new(),
        failing: Arc::clone(&failing),
    };
    let database = Builder::new()
        .create_with_backend(backend)
        .context("Failed to create the in-memory archive")?;
    let store = MessageStore::from_database(database, PathBuf::from("memory-archive"))?;
    store
        .bind(&EventHubIdentity {
            fully_qualified_namespace: "orders.servicebus.windows.net".to_owned(),
            event_hub_name: "orders".to_owned(),
            created_on: None,
        })
        .await?;
    Ok((store, failing))
}

/// Opens the archive file of `paths`, creating the data folder when missing,
/// with a writer thread that panics with "simulated writer bug" on its first
/// batch. The file holds the lock a real open would, and no archive tables
/// until a real open creates them.
///
/// # Errors
/// Fails when the data folder or the file cannot be opened, or the writer
/// thread cannot start.
pub fn store_with_dead_writer(paths: &DataPaths) -> anyhow::Result<MessageStore> {
    paths.ensure_base()?;
    let path = paths.archive_db();
    let database = Arc::new(open_database(&path, "archive")?);
    let writer = spawn_panicking(Arc::clone(&database), path.clone())?;
    Ok(MessageStore {
        inner: Arc::new(StoreInner {
            writer,
            database,
            path,
        }),
    })
}
