use super::schema::test_messages::message;
use super::tests::{orders_hub, store_one};
use super::writer::test_writers::spawn_panicking;
use super::*;
use redb::StorageBackend;
use redb::backends::InMemoryBackend;
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
    fn len(&self) -> Result<u64, std::io::Error> {
        self.inner.len()
    }

    fn read(&self, offset: u64, out: &mut [u8]) -> Result<(), std::io::Error> {
        self.inner.read(offset, out)
    }

    fn set_len(&self, len: u64) -> Result<(), std::io::Error> {
        self.check()?;
        self.inner.set_len(len)
    }

    fn sync_data(&self) -> Result<(), std::io::Error> {
        self.check()?;
        self.inner.sync_data()
    }

    fn write(&self, offset: u64, data: &[u8]) -> Result<(), std::io::Error> {
        self.check()?;
        self.inner.write(offset, data)
    }
}

/// A bound in-memory archive named `memory-archive` whose writes fail once
/// the returned flag is set.
pub(crate) async fn failing_store() -> (MessageStore, Arc<AtomicBool>) {
    let failing = Arc::new(AtomicBool::new(false));
    let backend = FailingBackend {
        inner: InMemoryBackend::new(),
        failing: Arc::clone(&failing),
    };
    let database = Builder::new().create_with_backend(backend).unwrap();
    let store = MessageStore::from_database(database, PathBuf::from("memory-archive")).unwrap();
    store.bind(&orders_hub()).await.unwrap();
    (store, failing)
}

#[tokio::test]
async fn a_failed_commit_fails_the_store_with_the_archive_path() {
    let (store, failing) = failing_store().await;
    failing.store(true, Ordering::SeqCst);

    let error = store_one(&store, &message("0", 1, "x"), true)
        .await
        .unwrap_err();

    let rendered = error.to_string();
    assert!(
        rendered.contains("Writing to the archive 'memory-archive' failed"),
        "{rendered}"
    );
    assert!(rendered.contains("simulated disk failure"), "{rendered}");
}

#[tokio::test]
async fn after_a_failed_commit_later_writes_fail_with_the_first_cause() {
    let (store, failing) = failing_store().await;
    failing.store(true, Ordering::SeqCst);
    store_one(&store, &message("0", 1, "x"), true)
        .await
        .unwrap_err();
    failing.store(false, Ordering::SeqCst);

    let error = store_one(&store, &message("0", 2, "y"), true)
        .await
        .unwrap_err();

    let rendered = error.to_string();
    assert!(rendered.contains("failed earlier"), "{rendered}");
    assert!(rendered.contains("simulated disk failure"), "{rendered}");
}

fn store_with_dead_writer(root: &tempfile::TempDir) -> MessageStore {
    let path = root.path().join("archive.redb");
    let database = Arc::new(open_database(&path, "archive").unwrap());
    let writer = spawn_panicking(Arc::clone(&database), path.clone()).unwrap();
    MessageStore {
        inner: Arc::new(StoreInner {
            writer,
            database,
            path,
        }),
    }
}

#[tokio::test]
async fn a_writer_that_panics_answers_its_batch_with_the_cause() {
    let root = tempfile::tempdir().unwrap();
    let store = store_with_dead_writer(&root);

    let error = store_one(&store, &message("0", 1, "x"), true)
        .await
        .unwrap_err();

    let rendered = error.to_string();
    assert!(rendered.contains("archive writer"), "{rendered}");
    assert!(rendered.contains("simulated writer bug"), "{rendered}");
}

#[tokio::test]
async fn after_the_writer_dies_later_stores_fail_with_its_cause() {
    let root = tempfile::tempdir().unwrap();
    let store = store_with_dead_writer(&root);
    store_one(&store, &message("0", 1, "x"), true)
        .await
        .unwrap_err();

    let error = store_one(&store, &message("0", 2, "y"), true)
        .await
        .unwrap_err();

    assert!(
        error.to_string().contains("simulated writer bug"),
        "{error:#}"
    );
}

#[tokio::test]
async fn closing_a_store_whose_writer_died_reports_the_cause() {
    let root = tempfile::tempdir().unwrap();
    let store = store_with_dead_writer(&root);
    store_one(&store, &message("0", 1, "x"), true)
        .await
        .unwrap_err();

    let error = store.close().await.unwrap_err();

    assert!(
        error.to_string().contains("simulated writer bug"),
        "{error:#}"
    );
}
