//! The disk-write modes: a sink that archives every message and also writes
//! the ones that pass the capture filters to text files, right away or when
//! the run finishes.

use crate::export::ExportOutcome;
use crate::export::render::format_full_message;
use crate::store::MessageStore;
use anyhow::{Context, anyhow};
use common_file_utils::permissions::{create_dir_all_owner_only, open_owner_only};
use common_utils::string_utils::sanitize_string_for_filename;
use serde::{Deserialize, Serialize};
use shared_eventhub_core::connection::EventHubIdentity;
use shared_eventhub_core::engine::{Checkpoint, MessageSink, StoreOutcome};
use shared_eventhub_core::filters::CaptureFilters;
use shared_eventhub_core::models::{CapturedMessage, MessageKey};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::watch;

/// When captured messages are also written to text files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiskWriteMode {
    /// Only the archive.
    #[default]
    Off,
    /// Each message as it is stored.
    Immediate,
    /// Every message of the run when [`WritingSink::finish`] is called.
    AtEnd,
}

/// Where and how message dumps are written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiskWriteConfig {
    pub mode: DiskWriteMode,
    /// The folder dumps go to; each day gets a `YYYY-MM-DD` subfolder.
    pub inbound_dir: PathBuf,
    /// Write only the body instead of the details block.
    pub content_only: bool,
}

/// A [`MessageSink`] that stores into a [`MessageStore`] and writes dumps of
/// newly stored messages that pass the current filters. Clones share the
/// filters and the at-end list.
#[derive(Clone)]
pub struct WritingSink {
    store: MessageStore,
    config: Arc<DiskWriteConfig>,
    filters: Arc<watch::Sender<CaptureFilters>>,
    pending: Arc<Mutex<Vec<MessageKey>>>,
}

impl WritingSink {
    pub fn new(store: MessageStore, config: DiskWriteConfig, filters: CaptureFilters) -> Self {
        Self {
            store,
            config: Arc::new(config),
            filters: Arc::new(watch::Sender::new(filters)),
            pending: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Replaces the filters a message must pass to be written; messages
    /// already written stay.
    pub fn set_filters(&self, filters: CaptureFilters) {
        self.filters.send_replace(filters);
    }

    /// Writes the run's dumps in at-end mode (nothing to do in the other
    /// modes), calling `progress` after each. A file that fails is counted and
    /// the rest are still written.
    ///
    /// # Errors
    /// Fails when the archive cannot be read; the messages stay queued, so a
    /// later call writes them all again.
    pub async fn finish(
        &self,
        mut progress: impl FnMut(&ExportOutcome) + Send + 'static,
    ) -> anyhow::Result<ExportOutcome> {
        match self.config.mode {
            DiskWriteMode::Off | DiskWriteMode::Immediate => Ok(ExportOutcome::default()),
            DiskWriteMode::AtEnd => {
                let keys = std::mem::take(&mut *self.pending()?);
                let retry = keys.clone();
                let store = self.store.clone();
                let config = Arc::clone(&self.config);
                let written = tokio::task::spawn_blocking(move || {
                    write_pending(&store, &config, &keys, &mut progress)
                })
                .await
                .context("Writing the at-end message files stopped unexpectedly")
                .and_then(|outcome| outcome);
                if written.is_err() {
                    let mut pending = self.pending()?;
                    let queued_since = std::mem::replace(&mut *pending, retry);
                    pending.extend(queued_since);
                }
                written
            }
        }
    }

    /// Closes the archive; see [`MessageStore::close`].
    ///
    /// # Errors
    /// Fails when other handles (other clones of this sink included) are
    /// still open, or the archive fails to close.
    pub async fn close(self) -> anyhow::Result<()> {
        self.store.close().await
    }

    fn pending(&self) -> anyhow::Result<MutexGuard<'_, Vec<MessageKey>>> {
        self.pending.lock().map_err(|_| {
            anyhow!("The list of messages to write at the end is unusable after a panic")
        })
    }
}

impl MessageSink for WritingSink {
    async fn bind(&self, hub: &EventHubIdentity) -> anyhow::Result<()> {
        self.store.bind(hub).await
    }

    async fn store(
        &self,
        message: &CapturedMessage,
        checkpoint: &Checkpoint,
        dedup: bool,
    ) -> anyhow::Result<StoreOutcome> {
        let outcome = self.store.store(message, checkpoint, dedup).await?;
        let passes = self.filters.borrow().matches(message);
        if !outcome.newly_stored || !passes {
            return Ok(outcome);
        }

        match self.config.mode {
            DiskWriteMode::Off => {}
            DiskWriteMode::Immediate => {
                let path = dump_path(&self.config.inbound_dir, message);
                let content = dump_content(message, self.config.content_only);
                tokio::task::spawn_blocking(move || write_dump(&path, &content))
                    .await
                    .context("Writing a message file stopped unexpectedly")??;
            }
            DiskWriteMode::AtEnd => self.pending()?.push(message.key.clone()),
        }
        Ok(outcome)
    }

    async fn load_checkpoint(&self, partition_id: &str) -> anyhow::Result<Option<Checkpoint>> {
        self.store.load_checkpoint(partition_id).await
    }
}

fn write_pending(
    store: &MessageStore,
    config: &DiskWriteConfig,
    keys: &[MessageKey],
    progress: &mut impl FnMut(&ExportOutcome),
) -> anyhow::Result<ExportOutcome> {
    let mut outcome = ExportOutcome::default();
    for key in keys {
        let message = store.get(key)?.with_context(|| {
            format!(
                "The message at partition {}, sequence {} is missing from the archive",
                key.partition_id, key.sequence_number
            )
        })?;
        let path = dump_path(&config.inbound_dir, &message);
        match write_dump(&path, &dump_content(&message, config.content_only)) {
            Ok(()) => outcome.exported += 1,
            Err(error) => outcome.record_failure(&path, &error),
        }
        progress(&outcome);
    }
    Ok(outcome)
}

/// `<inbound>/<YYYY-MM-DD>/<capture time>--<partition>-<sequence>.txt`, in
/// UTC.
fn dump_path(inbound_dir: &Path, message: &CapturedMessage) -> PathBuf {
    inbound_dir
        .join(message.processed_at.format("%Y-%m-%d").to_string())
        .join(format!(
            "{}--{}-{}.txt",
            message.processed_at.format("%Y-%m-%dT%H-%M-%S%.3f"),
            sanitize_string_for_filename(&message.key.partition_id),
            message.key.sequence_number
        ))
}

fn dump_content(message: &CapturedMessage, content_only: bool) -> String {
    if content_only {
        message.body.clone()
    } else {
        format_full_message(message)
    }
}

/// Writes a dump owner-only (0o600 in a 0o700 folder on Unix): dumps hold the
/// same bodies as the archive, even when the inbound folder is outside the
/// data folder.
fn write_dump(path: &Path, content: &str) -> anyhow::Result<()> {
    let write = || -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            create_dir_all_owner_only(parent).context("Failed to create the folder")?;
        }
        let mut options = OpenOptions::new();
        options.create(true).write(true).truncate(true);
        let mut file = open_owner_only(&mut options, path).context("Failed to open the file")?;
        file.write_all(content.as_bytes())
            .context("Failed to write the file")
    };

    write().with_context(|| format!("Failed to write the message file '{}'", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::DataPaths;
    use crate::store::schema::test_messages::{checkpoint_for, message};
    use shared_eventhub_core::filters::TextFilter;

    struct Fixture {
        root: tempfile::TempDir,
        sink: WritingSink,
    }

    impl Fixture {
        fn inbound(&self) -> PathBuf {
            self.root.path().join("inbound")
        }

        fn dumps(&self) -> Vec<String> {
            let mut names = Vec::new();
            if let Ok(days) = std::fs::read_dir(self.inbound()) {
                for day in days {
                    for file in std::fs::read_dir(day.unwrap().path()).unwrap() {
                        names.push(file.unwrap().file_name().to_string_lossy().into_owned());
                    }
                }
            }
            names.sort();
            names
        }
    }

    async fn fixture(mode: DiskWriteMode, filters: CaptureFilters) -> Fixture {
        let root = tempfile::tempdir().unwrap();
        let store = MessageStore::open(&DataPaths::new(root.path().join("data"))).unwrap();
        let sink = WritingSink::new(
            store,
            DiskWriteConfig {
                mode,
                inbound_dir: root.path().join("inbound"),
                content_only: true,
            },
            filters,
        );
        sink.bind(&EventHubIdentity {
            fully_qualified_namespace: "orders.servicebus.windows.net".to_owned(),
            event_hub_name: "orders".to_owned(),
        })
        .await
        .unwrap();
        Fixture { root, sink }
    }

    fn contains(text: &str) -> CaptureFilters {
        CaptureFilters {
            time: None,
            text: Some(TextFilter::Contains(vec![text.to_owned()])),
        }
    }

    async fn store(sink: &WritingSink, message: &CapturedMessage) -> anyhow::Result<StoreOutcome> {
        sink.store(message, &checkpoint_for(message), true).await
    }

    #[tokio::test]
    async fn immediate_writes_each_new_matching_message_once() {
        let fixture = fixture(DiskWriteMode::Immediate, contains("keep")).await;
        let kept = message("0", 1, "keep me");

        store(&fixture.sink, &kept).await.unwrap();
        store(&fixture.sink, &kept).await.unwrap();
        store(&fixture.sink, &message("0", 2, "drop me"))
            .await
            .unwrap();

        assert_eq!(
            fixture.dumps(),
            vec!["2026-07-02T12-30-01.000--0-1.txt".to_owned()]
        );
        let written = std::fs::read_to_string(
            fixture
                .inbound()
                .join("2026-07-02")
                .join("2026-07-02T12-30-01.000--0-1.txt"),
        )
        .unwrap();
        assert_eq!(written, "keep me");
    }

    #[tokio::test]
    async fn new_filters_change_what_is_written_next() {
        let fixture = fixture(DiskWriteMode::Immediate, contains("first")).await;

        fixture.sink.set_filters(contains("second"));
        store(&fixture.sink, &message("0", 1, "first"))
            .await
            .unwrap();
        store(&fixture.sink, &message("0", 2, "second"))
            .await
            .unwrap();

        assert_eq!(
            fixture.dumps(),
            vec!["2026-07-02T12-30-02.000--0-2.txt".to_owned()]
        );
    }

    #[tokio::test]
    async fn an_immediate_write_failure_fails_the_store_and_names_the_file() {
        let fixture = fixture(DiskWriteMode::Immediate, CaptureFilters::default()).await;
        std::fs::write(fixture.inbound(), "a file where the folder should be").unwrap();
        let stored = message("0", 1, "x");

        let error = store(&fixture.sink, &stored).await.unwrap_err();

        assert!(
            format!("{error:#}").contains("Failed to write the message file"),
            "{error:#}"
        );
        assert!(fixture.sink.store.get(&stored.key).unwrap().is_some());
    }

    #[tokio::test]
    async fn at_end_writes_nothing_until_finish_then_every_matching_message() {
        let fixture = fixture(DiskWriteMode::AtEnd, contains("keep")).await;
        store(&fixture.sink, &message("0", 1, "keep 1"))
            .await
            .unwrap();
        store(&fixture.sink, &message("0", 2, "skip"))
            .await
            .unwrap();
        store(&fixture.sink, &message("1", 3, "keep 3"))
            .await
            .unwrap();
        let before_finish = fixture.dumps();
        let progress_calls = Arc::new(Mutex::new(0_u32));
        let counter = Arc::clone(&progress_calls);

        let outcome = fixture
            .sink
            .finish(move |_| *counter.lock().unwrap() += 1)
            .await
            .unwrap();

        assert!(before_finish.is_empty());
        assert_eq!(outcome.exported, 2);
        assert_eq!(*progress_calls.lock().unwrap(), 2);
        assert_eq!(
            fixture.dumps(),
            vec![
                "2026-07-02T12-30-01.000--0-1.txt".to_owned(),
                "2026-07-02T12-30-03.000--1-3.txt".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn an_at_end_file_that_fails_is_counted_and_the_rest_are_written() {
        let fixture = fixture(DiskWriteMode::AtEnd, CaptureFilters::default()).await;
        store(&fixture.sink, &message("0", 1, "x")).await.unwrap();
        std::fs::create_dir_all(fixture.inbound()).unwrap();
        std::fs::write(
            fixture.inbound().join("2026-07-02"),
            "blocks the day folder",
        )
        .unwrap();

        let outcome = fixture.sink.finish(|_| {}).await.unwrap();

        assert_eq!(outcome.failed, 1);
        assert!(outcome.first_error.unwrap().contains("2026-07-02"));
    }

    #[tokio::test]
    async fn a_failed_finish_keeps_the_messages_for_the_next_one() {
        let fixture = fixture(DiskWriteMode::AtEnd, CaptureFilters::default()).await;
        let stored = message("0", 1, "x");
        store(&fixture.sink, &stored).await.unwrap();
        let transaction = fixture.sink.store.database().begin_write().unwrap();
        transaction
            .open_table(crate::store::schema::MESSAGES)
            .unwrap()
            .remove(("0", 1))
            .unwrap();
        transaction.commit().unwrap();

        let error = fixture.sink.finish(|_| {}).await.unwrap_err();

        assert!(
            error.to_string().contains("missing from the archive"),
            "{error:#}"
        );
        assert_eq!(*fixture.sink.pending.lock().unwrap(), vec![stored.key]);
    }

    #[tokio::test]
    async fn off_writes_nothing() {
        let fixture = fixture(DiskWriteMode::Off, CaptureFilters::default()).await;

        store(&fixture.sink, &message("0", 1, "x")).await.unwrap();
        let outcome = fixture.sink.finish(|_| {}).await.unwrap();

        assert!(fixture.dumps().is_empty());
        assert_eq!(outcome, ExportOutcome::default());
    }

    #[tokio::test]
    async fn a_poisoned_at_end_list_fails_the_store() {
        let fixture = fixture(DiskWriteMode::AtEnd, CaptureFilters::default()).await;
        let pending = Arc::clone(&fixture.sink.pending);
        std::thread::spawn(move || {
            let _guard = pending.lock().unwrap();
            panic!("poison the list");
        })
        .join()
        .unwrap_err();

        let error = store(&fixture.sink, &message("0", 1, "x"))
            .await
            .unwrap_err();

        assert!(
            error.to_string().contains("unusable after a panic"),
            "{error:#}"
        );
    }

    #[tokio::test]
    async fn closing_the_sink_closes_the_archive() {
        let fixture = fixture(DiskWriteMode::Off, CaptureFilters::default()).await;
        let paths = DataPaths::new(fixture.root.path().join("data"));

        fixture.sink.close().await.unwrap();

        assert!(MessageStore::open(&paths).is_ok());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dumps_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = fixture(DiskWriteMode::Immediate, CaptureFilters::default()).await;
        store(&fixture.sink, &message("0", 1, "x")).await.unwrap();

        let file = fixture
            .inbound()
            .join("2026-07-02")
            .join("2026-07-02T12-30-01.000--0-1.txt");
        let mode = std::fs::metadata(file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
