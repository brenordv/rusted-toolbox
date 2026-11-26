use super::*;
use eh_core::engine::{PartitionEnd, RunSummary};
use eh_core::stats::StatsSnapshot;
use eh_persistence::export::DiskWriteConfig;
use eh_persistence::paths::DataPaths;
use std::collections::BTreeMap;

fn summary(end: RunEnd, partitions: &[(&str, PartitionEnd)]) -> LoopEnd {
    LoopEnd::Finished(Ok(RunSummary {
        stats: StatsSnapshot::default(),
        partitions: partitions
            .iter()
            .map(|(id, end)| ((*id).to_owned(), end.clone()))
            .collect::<BTreeMap<_, _>>(),
        end,
    }))
}

fn clean() -> anyhow::Result<ExportOutcome> {
    Ok(ExportOutcome::default())
}

fn failed_files(count: u64) -> anyhow::Result<ExportOutcome> {
    Ok(ExportOutcome {
        failed: count,
        ..ExportOutcome::default()
    })
}

#[rstest::rstest]
#[case::completed(summary(RunEnd::Completed, &[("0", PartitionEnd::NothingLeftInWindow)]), RunOutcome::Completed)]
#[case::clean_stop(summary(RunEnd::Stopped, &[("0", PartitionEnd::Stopped)]), RunOutcome::Interrupted)]
#[case::before_start(LoopEnd::InterruptedBeforeStart, RunOutcome::Interrupted)]
fn clean_ends_map_to_completed_or_interrupted(
    #[case] ended: LoopEnd,
    #[case] expected: RunOutcome,
) {
    assert_eq!(run_outcome(ended, clean(), Ok(())).unwrap(), expected);
}

#[test]
fn a_partition_that_failed_before_the_stop_fails_the_run() {
    let ended = summary(
        RunEnd::Stopped,
        &[
            ("0", PartitionEnd::Stopped),
            ("1", PartitionEnd::Failed("recreated".to_owned())),
        ],
    );

    let error = run_outcome(ended, clean(), Ok(())).unwrap_err();

    assert!(
        error.to_string().contains("Partitions 1 failed"),
        "{error:#}"
    );
}

#[test]
fn a_failed_run_fails_with_its_reason() {
    let ended = summary(RunEnd::Failed("every partition failed".to_owned()), &[]);

    let error = run_outcome(ended, clean(), Ok(())).unwrap_err();

    assert!(
        error.to_string().contains("every partition failed"),
        "{error:#}"
    );
}

#[test]
fn a_run_that_cannot_start_returns_its_error() {
    let ended = LoopEnd::Finished(Err(anyhow!("bound to another event hub")));

    let error = run_outcome(ended, clean(), Ok(())).unwrap_err();

    assert_eq!(error.to_string(), "bound to another event hub");
}

#[test]
fn failed_message_files_fail_a_completed_run() {
    let ended = summary(RunEnd::Completed, &[]);

    let error = run_outcome(ended, failed_files(2), Ok(())).unwrap_err();

    assert!(error.to_string().contains("2 message file(s)"), "{error:#}");
}

#[test]
fn a_finish_error_fails_an_interrupted_run() {
    let error = run_outcome(
        LoopEnd::InterruptedBeforeStart,
        Err(anyhow!("archive unreadable")),
        Ok(()),
    )
    .unwrap_err();

    assert!(
        format!("{error:#}").contains("at-end message files: archive unreadable"),
        "{error:#}"
    );
}

#[test]
fn a_close_error_after_a_clean_stop_fails_the_run() {
    let ended = summary(RunEnd::Stopped, &[("0", PartitionEnd::Stopped)]);

    let error = run_outcome(ended, clean(), Err(anyhow!("handles still open"))).unwrap_err();

    assert!(
        format!("{error:#}").contains("Failed to close the archive: handles still open"),
        "{error:#}"
    );
}

#[test]
fn a_failed_run_a_finish_error_and_a_close_error_are_all_reported() {
    let ended = summary(RunEnd::Failed("partitions 1 failed".to_owned()), &[]);

    let error = run_outcome(
        ended,
        Err(anyhow!("archive unreadable")),
        Err(anyhow!("handles still open")),
    )
    .unwrap_err();

    let rendered = format!("{error:#}");
    assert!(rendered.contains("partitions 1 failed"), "{rendered}");
    assert!(rendered.contains("archive unreadable"), "{rendered}");
    assert!(
        rendered.contains("Failed to close the archive: handles still open"),
        "{rendered}"
    );
}

struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
        }
    }

    fn paths(&self) -> DataPaths {
        DataPaths::new(self.root.path().join(".eh-data"))
    }
}

#[tokio::test]
async fn at_end_mode_writes_the_pending_message_files_when_the_run_ends() {
    use eh_core::connection::EventHubIdentity;
    use eh_core::engine::{Checkpoint, MessageSink};
    use eh_core::models::{BodyKind, CapturedMessage, MessageKey};

    let fixture = Fixture::new();
    let inbound = fixture.root.path().join("inbound");
    let sink = WritingSink::new(
        MessageStore::open(&fixture.paths()).unwrap(),
        DiskWriteConfig {
            mode: DiskWriteMode::AtEnd,
            inbound_dir: inbound.clone(),
            content_only: true,
        },
        Default::default(),
    );
    sink.bind(&EventHubIdentity {
        fully_qualified_namespace: "orders.servicebus.windows.net".to_owned(),
        event_hub_name: "orders".to_owned(),
        created_on: None,
    })
    .await
    .unwrap();
    let now = chrono::Utc::now();
    let message = CapturedMessage {
        key: MessageKey {
            partition_id: "0".to_owned(),
            sequence_number: 1,
        },
        offset: None,
        partition_key: None,
        enqueued_time: Some(now),
        processed_at: now,
        message_id: None,
        correlation_id: None,
        content_type: None,
        properties: BTreeMap::new(),
        body: "at the end".to_owned(),
        body_kind: BodyKind::Utf8,
    };
    let checkpoint = Checkpoint {
        partition_id: "0".to_owned(),
        sequence_number: 1,
        offset: None,
        enqueued_time: Some(now),
        updated_at: now,
    };
    sink.store(&message, &checkpoint, true).await.unwrap();

    let outcome = finish_disk_writes(&sink, DiskWriteMode::AtEnd)
        .await
        .unwrap();
    sink.close().await.unwrap();

    assert_eq!(outcome.exported, 1);
    let day = std::fs::read_dir(&inbound)
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    let file = std::fs::read_dir(day.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    assert_eq!(std::fs::read_to_string(file.path()).unwrap(), "at the end");
}

#[tokio::test]
async fn the_other_disk_modes_have_nothing_to_finish() {
    let fixture = Fixture::new();
    let sink = WritingSink::new(
        MessageStore::open(&fixture.paths()).unwrap(),
        DiskWriteConfig {
            mode: DiskWriteMode::Immediate,
            inbound_dir: fixture.root.path().join("inbound"),
            content_only: false,
        },
        Default::default(),
    );

    let outcome = finish_disk_writes(&sink, DiskWriteMode::Immediate)
        .await
        .unwrap();
    sink.close().await.unwrap();

    assert_eq!(outcome, ExportOutcome::default());
}
