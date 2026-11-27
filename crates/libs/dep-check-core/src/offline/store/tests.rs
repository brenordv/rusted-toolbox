use super::*;
use crate::offline::testing::install;
use crate::test_support::osv_record;

const CARGO: Ecosystem = Ecosystem::Cargo;

async fn installed() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let records = [osv_record(
        "RUSTSEC-0001",
        "crates.io",
        "crate",
        "0",
        "1.0.0",
    )];
    install(dir.path(), CARGO, &records, Utc::now()).await;
    dir
}

/// A folder holding a file where `path` should be a file, so it can be neither opened for
/// writing nor deleted as a file.
fn block_with_a_folder(path: &std::path::Path) {
    std::fs::create_dir(path).unwrap();
    std::fs::write(path.join("inside"), b"x").unwrap();
}

#[tokio::test]
async fn a_lock_file_that_cannot_be_opened_reads_unavailable() {
    let dir = installed().await;
    let files = StoreFiles::of(dir.path(), CARGO);
    std::fs::remove_file(&files.lock).unwrap();
    block_with_a_folder(&files.lock);

    let status = status(dir.path(), CARGO);

    assert!(
        matches!(status, DataStatus::Unavailable { .. }),
        "{status:?}"
    );
}

#[tokio::test]
async fn a_meta_value_of_the_wrong_type_reads_unreadable() {
    let dir = installed().await;
    Connection::open(StoreFiles::of(dir.path(), CARGO).db)
        .unwrap()
        .execute("UPDATE meta SET archive_bytes = 'many'", [])
        .unwrap();

    let status = status(dir.path(), CARGO);

    assert!(
        matches!(status, DataStatus::Unreadable { .. }),
        "{status:?}"
    );
}

#[tokio::test]
async fn remove_fails_when_a_lock_file_cannot_be_opened() {
    let dir = installed().await;
    let files = StoreFiles::of(dir.path(), CARGO);
    std::fs::remove_file(&files.lock).unwrap();
    block_with_a_folder(&files.lock);

    let outcome = remove(dir.path(), CARGO);

    assert!(matches!(outcome, RemoveOutcome::Failed(_)), "{outcome:?}");
    assert!(files.db.exists());
}

#[tokio::test]
async fn remove_fails_when_the_update_lock_cannot_be_opened() {
    let dir = installed().await;
    let lock = update_lock(dir.path());
    std::fs::remove_file(&lock).unwrap();
    block_with_a_folder(&lock);

    let outcome = remove(dir.path(), CARGO);

    assert!(matches!(outcome, RemoveOutcome::Failed(_)), "{outcome:?}");
}

#[tokio::test]
async fn remove_fails_when_a_file_cannot_be_deleted() {
    let dir = installed().await;
    let files = StoreFiles::of(dir.path(), CARGO);
    block_with_a_folder(&files.part);

    let outcome = remove(dir.path(), CARGO);

    assert!(matches!(outcome, RemoveOutcome::Failed(_)), "{outcome:?}");
    assert!(!files.db.exists());
}

#[tokio::test]
async fn remove_deletes_the_store_s_files_and_reports_their_bytes() {
    let dir = installed().await;
    let files = StoreFiles::of(dir.path(), CARGO);
    std::fs::write(&files.part, b"a leftover download").unwrap();
    let before = file_bytes(&files).unwrap() + b"a leftover download".len() as u64;

    let outcome = remove(dir.path(), CARGO);

    assert_eq!(outcome, RemoveOutcome::Removed { bytes: before });
    for path in [&files.db, &files.wal, &files.shm, &files.part] {
        assert!(!path.exists(), "{}", path.display());
    }
    assert_eq!(status(dir.path(), CARGO), DataStatus::NotDownloaded);
}

#[tokio::test]
async fn remove_answers_busy_while_an_update_holds_its_lock() {
    let dir = installed().await;
    let _updating = locks::try_exclusive(&update_lock(dir.path())).unwrap();

    let outcome = remove(dir.path(), CARGO);

    assert_eq!(outcome, RemoveOutcome::Busy);
    assert!(StoreFiles::of(dir.path(), CARGO).db.exists());
}

#[tokio::test]
async fn remove_answers_in_use_while_a_check_reads_the_store() {
    let dir = installed().await;
    let OpenRead::Opened(reader) = open_read(dir.path(), CARGO) else {
        panic!("the store did not open");
    };

    let outcome = remove(dir.path(), CARGO);

    assert_eq!(outcome, RemoveOutcome::InUse);
    assert_eq!(reader.records_for("crate").unwrap().len(), 1);
}

#[test]
fn remove_deletes_a_leftover_download_with_no_store() {
    let dir = tempfile::tempdir().unwrap();
    let files = StoreFiles::of(dir.path(), CARGO);
    std::fs::write(&files.part, b"half a download").unwrap();

    let outcome = remove(dir.path(), CARGO);

    assert_eq!(
        outcome,
        RemoveOutcome::Removed {
            bytes: b"half a download".len() as u64
        }
    );
    assert!(!files.part.exists());
}

#[test]
fn remove_with_nothing_there_answers_not_downloaded() {
    let dir = tempfile::tempdir().unwrap();

    let in_a_folder = remove(dir.path(), CARGO);
    let with_no_folder = remove(&dir.path().join("missing"), CARGO);

    assert_eq!(
        (in_a_folder, with_no_folder),
        (RemoveOutcome::NotDownloaded, RemoveOutcome::NotDownloaded)
    );
}

#[tokio::test]
async fn a_store_whose_lock_a_removal_holds_reads_locked() {
    let dir = installed().await;
    let _removing = locks::try_exclusive(&StoreFiles::of(dir.path(), CARGO).lock).unwrap();

    assert_eq!(status(dir.path(), CARGO), DataStatus::Locked);
}

#[test]
fn a_file_that_is_not_a_database_reads_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        StoreFiles::of(dir.path(), CARGO).db,
        b"this is not an SQLite database, just some text long enough to be read as a header",
    )
    .unwrap();

    let status = status(dir.path(), CARGO);

    assert!(
        matches!(status, DataStatus::Unreadable { .. }),
        "{status:?}"
    );
}

#[test]
fn no_folder_at_all_reads_not_downloaded_and_creates_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("offline");

    assert_eq!(status(&missing, CARGO), DataStatus::NotDownloaded);
    assert!(!missing.exists());
}

#[tokio::test]
async fn a_failed_attempt_with_only_half_its_fields_reads_unreadable() {
    let dir = installed().await;
    Connection::open(StoreFiles::of(dir.path(), CARGO).db)
        .unwrap()
        .execute_batch(
            "CREATE TABLE meta_copy AS SELECT * FROM meta;
             DROP TABLE meta;
             CREATE TABLE meta AS SELECT * FROM meta_copy;
             UPDATE meta SET attempted_at = '2026-11-27T10:00:00Z';",
        )
        .unwrap();

    let status = status(dir.path(), CARGO);

    assert!(
        matches!(status, DataStatus::Unreadable { .. }),
        "{status:?}"
    );
}

#[tokio::test]
async fn a_date_that_does_not_parse_reads_unreadable() {
    let dir = installed().await;
    Connection::open(StoreFiles::of(dir.path(), CARGO).db)
        .unwrap()
        .execute("UPDATE meta SET current_at = 'yesterday'", [])
        .unwrap();

    let status = status(dir.path(), CARGO);

    assert!(
        matches!(status, DataStatus::Unreadable { .. }),
        "{status:?}"
    );
}

#[test]
fn each_store_file_is_named_from_its_ecosystem() {
    let dir = std::path::Path::new("offline");

    let files = StoreFiles::of(dir, Ecosystem::Npm);

    assert_eq!(
        [files.db, files.wal, files.shm, files.part, files.lock],
        [
            dir.join("npm.sqlite"),
            dir.join("npm.sqlite-wal"),
            dir.join("npm.sqlite-shm"),
            dir.join("npm.zip.part"),
            dir.join("npm.lock"),
        ]
    );
}
