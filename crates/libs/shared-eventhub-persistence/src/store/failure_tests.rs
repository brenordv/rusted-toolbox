use super::schema::test_messages::message;
use super::test_support::{failing_store, store_with_dead_writer};
use super::tests::store_one;
use super::*;
use std::sync::atomic::Ordering;

fn dead_writer_store(root: &tempfile::TempDir) -> MessageStore {
    store_with_dead_writer(&DataPaths::new(root.path().join("data"))).unwrap()
}

#[tokio::test]
async fn a_failed_commit_fails_the_store_with_the_archive_path() {
    let (store, failing) = failing_store().await.unwrap();
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
    let (store, failing) = failing_store().await.unwrap();
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

#[tokio::test]
async fn a_healthy_store_reports_no_write_failure() {
    let (store, _failing) = failing_store().await.unwrap();
    store_one(&store, &message("0", 1, "x"), true)
        .await
        .unwrap();

    assert_eq!(store.write_failure(), None);
}

#[tokio::test]
async fn a_failed_commit_is_reported_as_the_write_failure() {
    let (store, failing) = failing_store().await.unwrap();
    failing.store(true, Ordering::SeqCst);

    store_one(&store, &message("0", 1, "x"), true)
        .await
        .unwrap_err();

    let cause = store.write_failure().unwrap();
    assert!(cause.contains("simulated disk failure"), "{cause}");
}

#[tokio::test]
async fn a_writer_panic_is_reported_as_the_write_failure() {
    let root = tempfile::tempdir().unwrap();
    let store = dead_writer_store(&root);

    store_one(&store, &message("0", 1, "x"), true)
        .await
        .unwrap_err();

    assert_eq!(
        store.write_failure().as_deref(),
        Some("simulated writer bug")
    );
}

#[tokio::test]
async fn a_handle_is_the_last_one_only_while_no_clone_is_alive() {
    let (store, _failing) = failing_store().await.unwrap();
    let alone = store.is_last_handle();
    let clone = store.clone();
    let shared = store.is_last_handle();
    drop(clone);

    assert!(alone);
    assert!(!shared);
    assert!(store.is_last_handle());
}

#[tokio::test]
async fn a_writer_that_panics_answers_its_batch_with_the_cause() {
    let root = tempfile::tempdir().unwrap();
    let store = dead_writer_store(&root);

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
    let store = dead_writer_store(&root);
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
    let store = dead_writer_store(&root);
    store_one(&store, &message("0", 1, "x"), true)
        .await
        .unwrap_err();

    let error = store.close().await.unwrap_err();

    assert!(
        error.to_string().contains("simulated writer bug"),
        "{error:#}"
    );
}
