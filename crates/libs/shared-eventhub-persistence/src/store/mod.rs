//! The message archive: one redb database per data folder, bound to one event
//! hub, holding captured messages, a time index, per-partition checkpoints,
//! and the export log.

pub(crate) mod schema;
mod writer;

use crate::paths::DataPaths;
use anyhow::{Context, anyhow};
use chrono::{DateTime, Utc};
use common_file_utils::permissions::open_owner_only;
use redb::backends::FileBackend;
use redb::{
    Builder, Database, DatabaseError, ReadTransaction, ReadableDatabase, ReadableTable, TableHandle,
};
use schema::{
    BY_TIME, CHECKPOINTS, CREATED_ON_KEY, EVENT_HUB_KEY, EXPORT_LOG, MESSAGES, META, NAMESPACE_KEY,
    SCHEMA_VERSION, SCHEMA_VERSION_KEY, decode_checkpoint, decode_message, message_table_key,
};
use shared_eventhub_core::connection::EventHubIdentity;
use shared_eventhub_core::engine::{Checkpoint, MessageSink, StoreOutcome};
use shared_eventhub_core::models::{CapturedMessage, MessageKey};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};
use writer::{WriteRequest, WriterHandle};

/// A handle to an open archive. Clones share one database and one writer
/// thread; the archive file stays locked until the last handle is closed or
/// dropped.
///
/// The synchronous methods read the database directly and block; call them
/// from `spawn_blocking` when on an async runtime. Dropping the last handle
/// also blocks while the writer finishes; async callers use
/// [`close`](Self::close) instead.
#[derive(Clone)]
pub struct MessageStore {
    inner: Arc<StoreInner>,
}

/// The shared state. Field order is drop order: the writer stops (and
/// releases its database reference) before the database itself is dropped,
/// which flushes it and releases the file lock.
struct StoreInner {
    writer: WriterHandle,
    database: Arc<Database>,
    path: PathBuf,
}

impl MessageStore {
    /// Opens the data folder's archive, creating the folder and the file when
    /// missing, and starts its writer thread.
    ///
    /// # Errors
    /// Fails when the folder or file cannot be created, when another process
    /// (or another handle in this one) has the archive open, when the file is
    /// a redb database that is not an archive, or when it has a schema version
    /// this build does not read.
    pub fn open(paths: &DataPaths) -> anyhow::Result<Self> {
        paths.ensure_base()?;
        let path = paths.archive_db();
        let database = open_database(&path, "archive")?;

        Self::from_database(database, path)
    }

    fn from_database(database: Database, path: PathBuf) -> anyhow::Result<Self> {
        let created = prepare_schema(&database, &path)?;
        let database = Arc::new(database);
        let writer = WriterHandle::spawn(Arc::clone(&database), path.clone())?;
        info!(path = %path.display(), created, "Opened the archive");

        Ok(Self {
            inner: Arc::new(StoreInner {
                writer,
                database,
                path,
            }),
        })
    }

    /// Returns the message stored under `key`, or `None`.
    ///
    /// # Errors
    /// Fails when the archive cannot be read or the stored value does not
    /// decode.
    pub fn get(&self, key: &MessageKey) -> anyhow::Result<Option<CapturedMessage>> {
        let read = self.begin_read()?;
        let messages = read
            .open_table(MESSAGES)
            .context("Failed to open the archive's messages table")?;
        let stored = messages
            .get(message_table_key(key))
            .context("Failed to read a stored message")?;

        stored
            .map(|guard| decode_message(&key.partition_id, key.sequence_number, guard.value()))
            .transpose()
    }

    /// Stops the writer, closes the database, and releases the file lock,
    /// returning any error from closing. Only the last handle can close. When
    /// a checkpoint read whose caller stopped waiting still holds the
    /// database, this returns at once and the lock is released when that read
    /// finishes.
    ///
    /// # Errors
    /// Fails when other handles to the archive are still open (this handle is
    /// consumed either way), when the writer had died, or when redb reports an
    /// error while closing.
    pub async fn close(self) -> anyhow::Result<()> {
        let path = self.inner.path.clone();
        let inner = Arc::into_inner(self.inner).with_context(|| {
            format!(
                "Other handles to the archive '{}' are still open; close them first",
                path.display()
            )
        })?;

        tokio::task::spawn_blocking(move || inner.shut_down())
            .await
            .with_context(|| {
                format!(
                    "Closing the archive '{}' stopped unexpectedly",
                    path.display()
                )
            })?
    }

    /// Why the archive takes no more writes through this handle and its
    /// clones: the cause of the batch that failed, or of the writer thread's
    /// death. `None` while writes still work. Reads work either way.
    pub fn write_failure(&self) -> Option<String> {
        self.inner.writer.write_failure()
    }

    /// Whether no clone of this handle is alive, so [`close`](Self::close)
    /// on it would release the archive.
    pub fn is_last_handle(&self) -> bool {
        Arc::strong_count(&self.inner) == 1
    }

    /// The event hub the archive is bound to, or `None` before the first
    /// bind.
    pub(crate) fn binding(&self) -> anyhow::Result<Option<EventHubIdentity>> {
        let read = self.begin_read()?;
        let meta = read
            .open_table(META)
            .context("Failed to open the archive's meta table")?;

        read_binding(&meta)
    }

    #[cfg(test)]
    pub(crate) fn message_count(&self) -> anyhow::Result<u64> {
        use redb::ReadableTableMetadata;

        let read = self.begin_read()?;
        let messages = read.open_table(MESSAGES)?;

        Ok(messages.len()?)
    }

    pub(crate) fn begin_read(&self) -> anyhow::Result<ReadTransaction> {
        self.inner.database.begin_read().with_context(|| {
            format!(
                "Failed to start a read of the archive '{}'",
                self.inner.path.display()
            )
        })
    }

    pub(crate) fn database(&self) -> &Database {
        &self.inner.database
    }

    pub(crate) fn path(&self) -> &Path {
        &self.inner.path
    }

    async fn submit<T>(
        &self,
        (request, reply): (
            WriteRequest,
            tokio::sync::oneshot::Receiver<anyhow::Result<T>>,
        ),
    ) -> anyhow::Result<T> {
        self.inner.writer.submit(request)?;
        reply.await.map_err(|_| self.inner.writer.stopped_error())?
    }
}

impl StoreInner {
    fn shut_down(self) -> anyhow::Result<()> {
        let StoreInner {
            writer,
            database,
            path,
        } = self;
        writer.stop()?;
        match Arc::into_inner(database) {
            Some(database) => database
                .close()
                .with_context(|| format!("Failed to close the archive '{}'", path.display())),
            None => {
                // A checkpoint read abandoned by its caller still holds the
                // database; it closes (and releases the lock) when that read
                // finishes, which is moments away.
                warn!(
                    path = %path.display(),
                    "The archive is still being read; it closes when that read finishes"
                );
                Ok(())
            }
        }
    }
}

impl MessageSink for MessageStore {
    async fn bind(&self, hub: &EventHubIdentity) -> anyhow::Result<()> {
        self.submit(WriteRequest::bind(hub.clone())).await
    }

    async fn store(
        &self,
        message: &CapturedMessage,
        checkpoint: &Checkpoint,
        dedup: bool,
    ) -> anyhow::Result<StoreOutcome> {
        self.submit(WriteRequest::store(
            message.clone(),
            checkpoint.clone(),
            dedup,
        ))
        .await
    }

    /// Reads on a blocking thread that holds only the database, never a store
    /// handle, so a read whose caller stopped waiting never counts as an open
    /// handle.
    async fn load_checkpoint(&self, partition_id: &str) -> anyhow::Result<Option<Checkpoint>> {
        let database = Arc::clone(&self.inner.database);
        let path = self.inner.path.clone();
        let partition_id = partition_id.to_owned();

        tokio::task::spawn_blocking(move || read_checkpoint(&database, &path, &partition_id))
            .await
            .context("Reading a checkpoint stopped unexpectedly")?
    }
}

fn read_checkpoint(
    database: &Database,
    path: &Path,
    partition_id: &str,
) -> anyhow::Result<Option<Checkpoint>> {
    let read = database
        .begin_read()
        .with_context(|| format!("Failed to start a read of the archive '{}'", path.display()))?;
    let checkpoints = read
        .open_table(CHECKPOINTS)
        .context("Failed to open the archive's checkpoints table")?;
    let stored = checkpoints
        .get(partition_id)
        .context("Failed to read a checkpoint")?;

    stored
        .map(|guard| decode_checkpoint(partition_id, guard.value()))
        .transpose()
}

/// Reads the bound event hub from the archive's meta table.
pub(crate) fn read_binding(
    meta: &impl ReadableTable<&'static str, &'static str>,
) -> anyhow::Result<Option<EventHubIdentity>> {
    let namespace = meta
        .get(NAMESPACE_KEY)
        .context("Failed to read the archive's namespace")?
        .map(|guard| guard.value().to_owned());
    let event_hub = meta
        .get(EVENT_HUB_KEY)
        .context("Failed to read the archive's event hub")?
        .map(|guard| guard.value().to_owned());
    let created_on = meta
        .get(CREATED_ON_KEY)
        .context("Failed to read the archive's event hub creation time")?
        .map(|guard| {
            DateTime::parse_from_rfc3339(guard.value())
                .map(|time| time.with_timezone(&Utc))
                .context("The archive's event hub creation time could not be read")
        })
        .transpose()?;

    match (namespace, event_hub) {
        (Some(fully_qualified_namespace), Some(event_hub_name)) => Ok(Some(EventHubIdentity {
            fully_qualified_namespace,
            event_hub_name,
            created_on,
        })),
        (None, None) if created_on.is_none() => Ok(None),
        (None, None) | (Some(_), None) | (None, Some(_)) => {
            Err(anyhow!("The archive's event hub binding is incomplete"))
        }
    }
}

/// Opens (or creates) the redb file at `path` through an owner-only handle,
/// so a new file is 0o600 on Unix and an existing one is tightened before
/// redb reads it. `label` names the file in errors ("archive", "secrets
/// file").
pub(crate) fn open_database(path: &Path, label: &str) -> anyhow::Result<Database> {
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true).truncate(false);
    let file = open_owner_only(&mut options, path)
        .with_context(|| format!("Failed to open the {label} '{}'", path.display()))?;
    let backend = FileBackend::new(file).map_err(|error| open_error(path, label, error))?;

    Builder::new()
        .create_with_backend(backend)
        .map_err(|error| open_error(path, label, error))
}

/// What a locked archive or secrets file error says after naming the file.
const IN_USE: &str = "is in use by another eh or eh-gui (an eh-gui may still be finishing a \
                      session after its window closed). Close it and try again.";

fn open_error(path: &Path, label: &str, error: DatabaseError) -> anyhow::Error {
    match error {
        DatabaseError::DatabaseAlreadyOpen => {
            anyhow!("The {label} '{}' {IN_USE}", path.display())
        }
        other => anyhow::Error::new(other)
            .context(format!("Failed to open the {label} '{}'", path.display())),
    }
}

/// Creates the archive's tables in a new database, or checks that an existing
/// one is an archive this build reads. Returns `true` when it created them.
fn prepare_schema(database: &Database, path: &Path) -> anyhow::Result<bool> {
    let read = database
        .begin_read()
        .with_context(|| format!("Failed to read '{}'", path.display()))?;
    let tables: Vec<String> = read
        .list_tables()
        .with_context(|| format!("Failed to list the tables of '{}'", path.display()))?
        .map(|table| table.name().to_owned())
        .collect();
    let has_multimap_tables = read
        .list_multimap_tables()
        .with_context(|| format!("Failed to list the tables of '{}'", path.display()))?
        .next()
        .is_some();

    if tables.is_empty() && !has_multimap_tables {
        drop(read);
        create_schema(database, path)?;
        return Ok(true);
    }
    if !tables.iter().any(|name| name == META.name()) {
        anyhow::bail!("'{}' is a redb file but not an eh archive", path.display());
    }

    let meta = read
        .open_table(META)
        .with_context(|| format!("Failed to open the meta table of '{}'", path.display()))?;
    let version = meta
        .get(SCHEMA_VERSION_KEY)
        .with_context(|| format!("Failed to read the schema version of '{}'", path.display()))?
        .map(|guard| guard.value().to_owned());
    match version.as_deref() {
        Some(SCHEMA_VERSION) => Ok(false),
        Some(other) => anyhow::bail!(
            "The archive '{}' has schema version {other}; this build reads version \
             {SCHEMA_VERSION} (written by a newer eh?)",
            path.display()
        ),
        None => anyhow::bail!(
            "'{}' has no schema version, so it is not an eh archive",
            path.display()
        ),
    }
}

fn create_schema(database: &Database, path: &Path) -> anyhow::Result<()> {
    let transaction = database
        .begin_write()
        .with_context(|| format!("Failed to initialize '{}'", path.display()))?;
    {
        let mut meta = transaction.open_table(META)?;
        meta.insert(SCHEMA_VERSION_KEY, SCHEMA_VERSION)?;
        transaction.open_table(MESSAGES)?;
        transaction.open_table(BY_TIME)?;
        transaction.open_table(CHECKPOINTS)?;
        transaction.open_table(EXPORT_LOG)?;
    }

    transaction
        .commit()
        .with_context(|| format!("Failed to initialize '{}'", path.display()))
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
mod failure_tests;
#[cfg(test)]
mod tests;
