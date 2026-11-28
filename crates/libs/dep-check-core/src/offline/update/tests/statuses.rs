//! How an update treats each status it finds the data in: an unreadable store rebuilt only once
//! its download is complete, a store with no `meta` row installed, a newer store never touched,
//! and a busy one left alone.

use super::*;

fn second_records() -> [Value; 1] {
    [osv_record(
        "RUSTSEC-0009",
        "crates.io",
        "first-crate",
        "0",
        "9.0.0",
    )]
}

/// A Ready store of [`first_records`] made unreadable by an unknown format.
async fn unreadable_store() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), CARGO, &first_records(), now()).await;
    raw(dir.path(), CARGO)
        .execute("UPDATE meta SET format = -1", [])
        .unwrap();
    dir
}

#[tokio::test]
async fn an_unreadable_store_is_rebuilt_once_its_download_is_complete() {
    let dir = unreadable_store().await;

    let outcome = install(dir.path(), CARGO, &second_records(), now()).await;

    assert!(matches!(
        outcome,
        UpdateOutcome::Installed { records: 1, .. }
    ));
    assert_eq!(stored_ids(dir.path(), CARGO), ["RUSTSEC-0009"]);
}

#[tokio::test]
async fn an_unreadable_store_whose_download_fails_keeps_its_files() {
    let dir = unreadable_store().await;
    let server = MockServer::start();
    server.route("GET", &archive_path(CARGO), Reply::new(500, ""));

    let outcome = update_manually(&updater_at(server.base(), dir.path(), now()), CARGO).await;

    assert_eq!(outcome, UpdateOutcome::Failed(UpdateFailure::Status(500)));
    assert_eq!(stored_ids(dir.path(), CARGO), ["RUSTSEC-0001"]);
    assert!(matches!(
        store::status(dir.path(), CARGO),
        DataStatus::Unreadable { .. }
    ));
}

#[tokio::test]
async fn an_unreadable_store_a_check_holds_is_not_rebuilt() {
    let dir = unreadable_store().await;
    let reading = locks::try_shared(&files(&dir).lock).unwrap();

    let outcome = install(dir.path(), CARGO, &second_records(), now()).await;

    assert_eq!(outcome, UpdateOutcome::Failed(UpdateFailure::InUse));
    drop(reading);
    assert_eq!(stored_ids(dir.path(), CARGO), ["RUSTSEC-0001"]);
}

#[tokio::test]
async fn a_store_with_no_meta_row_reads_not_downloaded_and_installs() {
    let dir = tempfile::tempdir().unwrap();
    raw(dir.path(), CARGO)
        .execute_batch("CREATE TABLE leftover (x INTEGER);")
        .unwrap();

    let before = store::status(dir.path(), CARGO);
    let outcome = install(dir.path(), CARGO, &first_records(), now()).await;

    assert_eq!(before, DataStatus::NotDownloaded);
    assert!(matches!(outcome, UpdateOutcome::Installed { .. }));
}

#[tokio::test]
async fn a_store_saved_by_a_newer_dep_check_is_never_updated() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), CARGO, &first_records(), now()).await;
    raw(dir.path(), CARGO)
        .execute("UPDATE meta SET format = 2", [])
        .unwrap();
    let server = MockServer::start();

    let outcome = update_manually(&updater_at(server.base(), dir.path(), now()), CARGO).await;

    assert_eq!(store::status(dir.path(), CARGO), DataStatus::NewerFormat);
    assert_eq!(outcome, UpdateOutcome::Failed(UpdateFailure::NewerFormat));
    assert!(server.received().is_empty());
}

#[tokio::test]
async fn a_store_not_in_wal_mode_reads_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), CARGO, &first_records(), now()).await;
    let mode: String = raw(dir.path(), CARGO)
        .query_row("PRAGMA journal_mode = DELETE", [], |row| row.get(0))
        .unwrap();

    let status = store::status(dir.path(), CARGO);

    assert_eq!(mode, "delete");
    assert!(
        matches!(status, DataStatus::Unreadable { .. }),
        "{status:?}"
    );
}

#[tokio::test]
async fn a_busy_store_reads_unavailable_and_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), CARGO, &first_records(), now()).await;
    let holder = raw(dir.path(), CARGO);
    holder
        .execute_batch(
            "PRAGMA locking_mode = EXCLUSIVE; BEGIN IMMEDIATE; UPDATE meta SET records = records;",
        )
        .unwrap();
    let server = MockServer::start();

    let status = store::status(dir.path(), CARGO);
    let outcome = update_manually(&updater_at(server.base(), dir.path(), now()), CARGO).await;

    assert!(
        matches!(status, DataStatus::Unavailable { .. }),
        "{status:?}"
    );
    assert_eq!(outcome, UpdateOutcome::Failed(UpdateFailure::InUse));
    assert!(server.received().is_empty());
    holder.execute_batch("ROLLBACK").unwrap();
    drop(holder);
    assert_eq!(stored_ids(dir.path(), CARGO), ["RUSTSEC-0001"]);
}
