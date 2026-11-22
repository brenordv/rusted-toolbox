use super::*;
use crate::paths::DataPaths;
use crate::store::schema::test_messages::{checkpoint_for, message};
use shared_eventhub_core::connection::EventHubIdentity;
use shared_eventhub_core::engine::MessageSink;

async fn archive(count: i64) -> (tempfile::TempDir, MessageStore, Vec<CapturedMessage>) {
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
    let messages: Vec<CapturedMessage> = (1..=count)
        .map(|sequence| message("0", sequence, &format!("body {sequence}")))
        .collect();
    for message in &messages {
        store
            .store(message, &checkpoint_for(message), true)
            .await
            .unwrap();
    }
    (root, store, messages)
}

fn options(root: &tempfile::TempDir, format: ExportFormat, condense: bool) -> ExportOptions {
    ExportOptions {
        format,
        include_metadata: true,
        condense_output: condense,
        use_local_time: false,
        export_folder: root.path().join("export"),
        skip_already_exported: true,
    }
}

fn run(store: &MessageStore, options: &ExportOptions) -> ExportOutcome {
    store
        .export(
            &SearchQuery::default(),
            options,
            |_| {},
            &AtomicBool::new(false),
        )
        .unwrap()
}

#[tokio::test]
async fn individual_exports_land_in_dated_folders_named_by_partition_and_sequence() {
    let (root, store, _) = archive(1).await;

    let outcome = run(&store, &options(&root, ExportFormat::Txt, false));

    assert_eq!(outcome.exported, 1);
    assert!(!outcome.cancelled);
    assert!(
        root.path()
            .join("export")
            .join("2026-07")
            .join("02")
            .join("2026-07-02-T-12-30-01-000000000-0-1.txt")
            .exists()
    );
}

#[tokio::test]
async fn condensed_exports_write_one_file_per_month() {
    let (root, store, _) = archive(3).await;

    run(&store, &options(&root, ExportFormat::Csv, true));

    let csv = std::fs::read_to_string(
        root.path()
            .join("export")
            .join("2026-07")
            .join("messages-2026-07.csv"),
    )
    .unwrap();
    assert_eq!(csv.lines().count(), 4);
    assert!(csv.lines().nth(1).unwrap().starts_with("orders,0,1,"));
}

#[tokio::test]
async fn a_second_export_skips_what_the_log_already_holds() {
    let (root, store, _) = archive(3).await;
    let options = options(&root, ExportFormat::Txt, true);
    run(&store, &options);

    let second = run(&store, &options);

    assert_eq!((second.exported, second.skipped), (0, 3));
}

#[tokio::test]
async fn a_deleted_individual_file_is_exported_again() {
    let (root, store, _) = archive(3).await;
    let options = options(&root, ExportFormat::Txt, false);
    run(&store, &options);
    std::fs::remove_file(
        root.path()
            .join("export")
            .join("2026-07")
            .join("02")
            .join("2026-07-02-T-12-30-02-000000000-0-2.txt"),
    )
    .unwrap();

    let second = run(&store, &options);

    assert_eq!((second.exported, second.skipped), (1, 2));
}

#[tokio::test]
async fn without_skipping_everything_is_exported_again() {
    let (root, store, _) = archive(3).await;
    let mut options = options(&root, ExportFormat::Txt, false);
    run(&store, &options);
    options.skip_already_exported = false;

    let second = run(&store, &options);

    assert_eq!((second.exported, second.skipped), (3, 0));
}

#[tokio::test]
async fn the_export_log_is_shared_by_every_format() {
    let (root, store, _) = archive(2).await;
    run(&store, &options(&root, ExportFormat::Csv, true));

    let as_json = run(&store, &options(&root, ExportFormat::Json, true));

    assert_eq!((as_json.exported, as_json.skipped), (0, 2));
}

#[tokio::test]
async fn failed_files_are_counted_and_the_run_continues() {
    let (root, store, _) = archive(25).await;
    let mut options = options(&root, ExportFormat::Txt, false);
    let blocker = root.path().join("blocker");
    std::fs::write(&blocker, "a file where the export folder should be").unwrap();
    options.export_folder = blocker;

    let outcome = run(&store, &options);

    assert_eq!((outcome.exported, outcome.failed), (0, 25));
    assert!(outcome.first_error.unwrap().contains("blocker"));
}

#[rstest::rstest]
#[case::json(ExportFormat::Json)]
#[case::csv(ExportFormat::Csv)]
#[tokio::test]
async fn a_cancelled_export_leaves_complete_files(#[case] format: ExportFormat) {
    let (root, store, _) = archive(5).await;
    let options = options(&root, format, true);
    let cancel = AtomicBool::new(false);

    let outcome = store
        .export(
            &SearchQuery::default(),
            &options,
            |progress| {
                if progress.exported == 2 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
            &cancel,
        )
        .unwrap();

    assert_eq!(outcome.exported, 2);
    assert!(outcome.cancelled);
    let written = std::fs::read_to_string(
        root.path()
            .join("export")
            .join("2026-07")
            .join(format!("messages-2026-07.{}", format.extension())),
    )
    .unwrap();
    let rows = match format {
        ExportFormat::Json => serde_json::from_str::<Vec<serde_json::Value>>(&written)
            .unwrap()
            .len(),
        ExportFormat::Csv => csv::Reader::from_reader(written.as_bytes())
            .records()
            .map(Result::unwrap)
            .count(),
        ExportFormat::Txt => unreachable!("only json and csv are cancelled here"),
    };
    assert_eq!(rows, 2);
}

#[tokio::test]
async fn an_export_runs_while_capture_keeps_storing() {
    let (root, store, _) = archive(50).await;
    let writer = store.clone();
    let capture = tokio::spawn(async move {
        for sequence in 51..=250 {
            let next = message("1", sequence, "live");
            writer
                .store(&next, &checkpoint_for(&next), true)
                .await
                .unwrap();
        }
    });
    let exporter = store.clone();
    let options = options(&root, ExportFormat::Json, true);

    let outcome = tokio::task::spawn_blocking(move || run(&exporter, &options))
        .await
        .unwrap();
    capture.await.unwrap();

    assert!(outcome.exported >= 50);
    assert_eq!(outcome.failed, 0);
}

#[tokio::test]
async fn export_one_writes_a_copy_and_leaves_the_log_alone() {
    let (root, store, messages) = archive(1).await;
    let destination = root.path().join("copies").join("chosen name.json");
    std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
    std::fs::write(&destination, "old").unwrap();
    let options = options(&root, ExportFormat::Json, true);

    store
        .export_one(&messages[0].key, &options, &destination)
        .unwrap();
    let full_run = run(&store, &options);

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&destination).unwrap()).unwrap();
    assert_eq!(written["message_content"], "body 1");
    assert_eq!(full_run.exported, 1);
}

#[tokio::test]
async fn an_export_log_that_cannot_be_written_fails_the_export() {
    let (store, failing) = crate::store::test_support::failing_store().await.unwrap();
    let stored = message("0", 1, "x");
    store
        .store(&stored, &checkpoint_for(&stored), true)
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    failing.store(true, std::sync::atomic::Ordering::SeqCst);

    let error = store
        .export(
            &SearchQuery::default(),
            &options(&root, ExportFormat::Txt, false),
            |_| {},
            &AtomicBool::new(false),
        )
        .unwrap_err();

    assert!(format!("{error:#}").contains("export log"), "{error:#}");
}

#[tokio::test]
async fn export_one_of_a_missing_key_fails() {
    let (root, store, _) = archive(0).await;
    let missing = MessageKey {
        partition_id: "0".to_owned(),
        sequence_number: 99,
    };

    let error = store
        .export_one(
            &missing,
            &options(&root, ExportFormat::Txt, false),
            &root.path().join("x.txt"),
        )
        .unwrap_err();

    assert!(error.to_string().contains("sequence 99"), "{error:#}");
}
