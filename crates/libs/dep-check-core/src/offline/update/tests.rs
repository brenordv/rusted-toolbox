use super::*;
use crate::offline::store::{OpenRead, open_read};
use crate::offline::testing::{
    archive_path, install, not_cancelled, quiet, raw, stored_ids, test_limits, update_manually,
    updater_at, updater_with,
};
use crate::test_support::{MockServer, Reply, closed_port, osv_record, osv_zip, zip_of};
use chrono::TimeDelta;
use serde_json::{Value, json};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

mod failures;
mod statuses;

const CARGO: Ecosystem = Ecosystem::Cargo;

fn now() -> DateTime<Utc> {
    Utc::now()
}

fn first_records() -> Vec<Value> {
    vec![osv_record(
        "RUSTSEC-0001",
        "crates.io",
        "first-crate",
        "0.1.0",
        "1.0.0",
    )]
}

fn files(dir: &tempfile::TempDir) -> StoreFiles {
    StoreFiles::of(dir.path(), CARGO)
}

fn meta_of(dir: &tempfile::TempDir) -> StoreMeta {
    match store::status(dir.path(), CARGO) {
        DataStatus::Ready { meta, .. } => meta,
        other => panic!("not ready: {other:?}"),
    }
}

fn recording() -> (ProgressFn, Arc<Mutex<Vec<UpdateProgress>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&events);
    let progress: ProgressFn = Arc::new(move |event| seen.lock().unwrap().push(event));
    (progress, events)
}

fn index_rows(dir: &tempfile::TempDir) -> Vec<(String, String)> {
    let conn = raw(dir.path(), CARGO);
    let mut statement = conn
        .prepare("SELECT name, id FROM packages ORDER BY name, id")
        .unwrap();
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn stored_record(dir: &tempfile::TempDir, id: &str) -> Value {
    let text: String = raw(dir.path(), CARGO)
        .query_row("SELECT record FROM records WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn an_install_stores_slim_records_under_normalized_names() {
    let dir = tempfile::tempdir().unwrap();
    let mut first = osv_record("RUSTSEC-0001", "crates.io", "Serde_JSON", "0.1.0", "1.0.0");
    first["details"] = json!("a long story");
    first["credits"] = json!([{"name": "someone"}]);
    let two_crates = json!({
        "id": "GHSA-0002",
        "affected": [
            {"package": {"ecosystem": "crates.io", "name": "crate-a"}, "versions": ["1.0.0"]},
            {"package": {"ecosystem": "npm", "name": "crate-a"}, "versions": ["1.0.0"]},
            {"package": {"ecosystem": "crates.io", "name": "crate-b"}, "versions": ["2.0.0"]}
        ]
    });
    let mut withdrawn = osv_record("RUSTSEC-0003", "crates.io", "gone", "0", "1.0.0");
    withdrawn["withdrawn"] = json!("2026-01-01T00:00:00Z");
    let npm_only = osv_record("GHSA-0004", "npm", "left-pad", "0", "1.0.0");
    let records = [first, two_crates, withdrawn, npm_only];
    let archive = osv_zip(&records);
    let at = now();

    let outcome = install(dir.path(), CARGO, &records, at).await;

    assert_eq!(
        outcome,
        UpdateOutcome::Installed {
            records: 2,
            archive_bytes: archive.len() as u64
        }
    );
    assert_eq!(
        index_rows(&dir),
        [
            ("crate-a".to_owned(), "GHSA-0002".to_owned()),
            ("crate-b".to_owned(), "GHSA-0002".to_owned()),
            ("serde-json".to_owned(), "RUSTSEC-0001".to_owned()),
        ]
    );
    let slim = stored_record(&dir, "RUSTSEC-0001");
    assert!(slim.get("details").is_none() && slim.get("credits").is_none());
    assert_eq!(slim["summary"], "summary of RUSTSEC-0001");
    let kept = stored_record(&dir, "GHSA-0002");
    assert_eq!(kept["affected"].as_array().unwrap().len(), 2);
    let meta = meta_of(&dir);
    assert_eq!(
        (
            meta.records,
            meta.etag.as_deref(),
            meta.current_at,
            meta.downloaded_at
        ),
        (2, Some("\"first\""), at, at)
    );
    assert!(meta.last_failure.is_none());
    assert!(!files(&dir).part.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn an_install_creates_its_folders_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join("dep-check").join("offline");

    install(&dir, CARGO, &first_records(), now()).await;

    for folder in [home.path().join("dep-check"), dir] {
        let mode = std::fs::metadata(&folder).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "{}", folder.display());
    }
}

#[tokio::test]
async fn leftover_journals_beside_no_store_are_deleted_before_the_store_is_made() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(files(&dir).wal, b"marker-wal").unwrap();
    std::fs::write(files(&dir).journal, b"marker-journal").unwrap();

    let outcome = install(dir.path(), CARGO, &first_records(), now()).await;

    assert!(matches!(outcome, UpdateOutcome::Installed { .. }));
    let wal = std::fs::read(files(&dir).wal).unwrap_or_default();
    assert!(!wal.starts_with(b"marker-wal"));
    assert!(!files(&dir).journal.exists());
    assert!(matches!(
        store::status(dir.path(), CARGO),
        DataStatus::Ready { .. }
    ));
}

#[tokio::test]
async fn an_install_clears_the_note_an_earlier_failure_left() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), CARGO, &first_records(), now()).await;
    store::write_attempt(
        &files(&dir).db,
        now(),
        "storage.googleapis.com answered 503",
    )
    .unwrap();
    let noted = meta_of(&dir).last_failure;

    install(dir.path(), CARGO, &first_records(), now()).await;

    assert!(noted.is_some());
    assert!(meta_of(&dir).last_failure.is_none());
}

#[tokio::test]
async fn an_automatic_update_touches_only_ready_data_that_is_due() {
    let fresh = tempfile::tempdir().unwrap();
    install(fresh.path(), CARGO, &first_records(), now()).await;
    let unreadable = tempfile::tempdir().unwrap();
    install(
        unreadable.path(),
        CARGO,
        &first_records(),
        now() - TimeDelta::days(2),
    )
    .await;
    raw(unreadable.path(), CARGO)
        .execute("UPDATE meta SET format = -1", [])
        .unwrap();
    let empty = tempfile::tempdir().unwrap();
    let server = MockServer::start();
    server.route(
        "GET",
        &archive_path(CARGO),
        Reply::new(200, osv_zip(&first_records())),
    );
    let auto = |dir: &tempfile::TempDir| {
        let updater = updater_at(server.base(), dir.path(), now());
        async move {
            updater
                .update(CARGO, Trigger::Auto, quiet(), not_cancelled())
                .await
        }
    };

    let outcomes = [
        auto(&fresh).await,
        auto(&unreadable).await,
        auto(&empty).await,
    ];

    assert_eq!(
        outcomes,
        [
            UpdateOutcome::Skipped(Skip::NotDue),
            UpdateOutcome::Skipped(Skip::Unreadable),
            UpdateOutcome::Skipped(Skip::NotDownloaded),
        ]
    );
    assert!(server.received().is_empty());
    assert_eq!(stored_ids(unreadable.path(), CARGO), ["RUSTSEC-0001"]);
    assert_eq!(
        store::status(empty.path(), CARGO),
        DataStatus::NotDownloaded
    );
}

#[tokio::test]
async fn the_update_lock_stays_taken_while_an_install_outlives_its_dropped_update() {
    let dir = tempfile::tempdir().unwrap();
    install(
        dir.path(),
        CARGO,
        &first_records(),
        now() - TimeDelta::days(2),
    )
    .await;
    let server = MockServer::start();
    server.route(
        "GET",
        &archive_path(CARGO),
        Reply::new(200, osv_zip(&first_records())),
    );
    let (installing, started) = std::sync::mpsc::channel::<()>();
    let installing = Mutex::new(installing);
    let progress: ProgressFn = Arc::new(move |event| {
        if event == (UpdateProgress::Installing { done: 0, total: 1 }) {
            installing.lock().unwrap().send(()).unwrap();
            std::thread::sleep(Duration::from_millis(600));
        }
    });
    let updater = updater_at(server.base(), dir.path(), now());
    let task = tokio::spawn(async move {
        updater
            .update(CARGO, Trigger::Manual, progress, not_cancelled())
            .await
    });
    tokio::task::spawn_blocking(move || started.recv().unwrap())
        .await
        .unwrap();

    task.abort();
    let during = store::remove(dir.path(), CARGO);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let after = store::remove(dir.path(), CARGO);

    assert_eq!(during, store::RemoveOutcome::Busy);
    assert!(
        matches!(after, store::RemoveOutcome::Removed { .. }),
        "{after:?}"
    );
}

#[tokio::test]
async fn a_reader_keeps_its_snapshot_through_an_install_and_then_sees_the_new_rows() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), CARGO, &first_records(), now()).await;
    let OpenRead::Opened(reader) = open_read(dir.path(), CARGO) else {
        panic!("the store did not open");
    };
    let second = [osv_record(
        "RUSTSEC-0009",
        "crates.io",
        "first-crate",
        "0",
        "9.0.0",
    )];

    let outcome = install(dir.path(), CARGO, &second, now()).await;

    assert!(matches!(outcome, UpdateOutcome::Installed { .. }));
    let old = reader.records_for("first-crate").unwrap();
    assert!(old[0].as_deref().unwrap().contains("RUSTSEC-0001"));
    drop(reader);
    let OpenRead::Opened(reader) = open_read(dir.path(), CARGO) else {
        panic!("the store did not open again");
    };
    let new = reader.records_for("first-crate").unwrap();
    assert!(new[0].as_deref().unwrap().contains("RUSTSEC-0009"));
}

#[tokio::test]
async fn an_unchanged_copy_is_confirmed_current_and_clears_the_last_failure() {
    let dir = tempfile::tempdir().unwrap();
    let server = MockServer::start();
    let path = archive_path(CARGO);
    server.route(
        "GET",
        &path,
        Reply::new(200, osv_zip(&first_records())).with_header("etag", "\"v1\""),
    );
    server.route("GET", &path, Reply::new(500, ""));
    server.route("GET", &path, Reply::new(304, ""));
    let installed_at = now() - TimeDelta::days(8);
    update_manually(&updater_at(server.base(), dir.path(), installed_at), CARGO).await;
    update_manually(&updater_at(server.base(), dir.path(), now()), CARGO).await;
    let confirmed_at = now();

    let outcome =
        update_manually(&updater_at(server.base(), dir.path(), confirmed_at), CARGO).await;

    assert_eq!(outcome, UpdateOutcome::Unchanged);
    let meta = meta_of(&dir);
    assert_eq!(
        (meta.current_at, meta.downloaded_at, meta.last_failure),
        (confirmed_at, installed_at, None)
    );
    assert_eq!(stored_ids(dir.path(), CARGO), ["RUSTSEC-0001"]);
    let sent = server.received_at(&path);
    assert_eq!(sent[2].header("if-none-match"), Some("\"v1\""));
    assert!(!files(&dir).part.exists());
}

#[tokio::test]
async fn a_second_updater_answers_busy_and_touches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(files(&dir).part, b"the first updater's download").unwrap();
    let held = locks::try_exclusive(&store::update_lock(dir.path())).unwrap();
    let server = MockServer::start();
    server.route(
        "GET",
        &archive_path(CARGO),
        Reply::new(200, osv_zip(&first_records())),
    );
    let updater = updater_at(server.base(), dir.path(), now());
    let (progress, events) = recording();

    let outcome = updater
        .update(CARGO, Trigger::Manual, progress, not_cancelled())
        .await;

    assert_eq!(outcome, UpdateOutcome::Busy);
    assert!(server.received().is_empty());
    assert!(events.lock().unwrap().is_empty());
    assert_eq!(
        std::fs::read(files(&dir).part).unwrap(),
        b"the first updater's download"
    );
    drop(held);
    assert!(matches!(
        update_manually(&updater, CARGO).await,
        UpdateOutcome::Installed { .. }
    ));
}

fn cancelling_at(
    stage: fn(&UpdateProgress) -> bool,
) -> (ProgressFn, Arc<AtomicBool>, Arc<Mutex<Vec<UpdateProgress>>>) {
    let cancel = not_cancelled();
    let events = Arc::new(Mutex::new(Vec::new()));
    let (flag, seen) = (Arc::clone(&cancel), Arc::clone(&events));
    let progress: ProgressFn = Arc::new(move |event| {
        if stage(&event) {
            flag.store(true, Ordering::SeqCst);
        }
        seen.lock().unwrap().push(event);
    });
    (progress, cancel, events)
}

async fn cancelled_update(stage: fn(&UpdateProgress) -> bool, body: Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let at = now() - TimeDelta::hours(30);
    install(dir.path(), CARGO, &first_records(), at).await;
    let server = MockServer::start();
    server.route("GET", &archive_path(CARGO), Reply::new(200, body));
    let limits = UpdateLimits {
        max_archive_bytes: 64 * 1024 * 1024,
        max_entry_bytes: 8 * 1024 * 1024,
        ..test_limits()
    };
    let updater = updater_with(server.base(), dir.path(), now(), limits);
    let (progress, cancel, events) = cancelling_at(stage);

    let outcome = updater
        .update(CARGO, Trigger::Manual, progress, cancel)
        .await;

    assert_eq!(outcome, UpdateOutcome::Cancelled { kept: Some(at) });
    assert_eq!(stored_ids(dir.path(), CARGO), ["RUSTSEC-0001"]);
    assert!(meta_of(&dir).last_failure.is_none());
    assert!(!files(&dir).part.exists());
    assert_eq!(events.lock().unwrap().last(), Some(&UpdateProgress::Ending));
}

#[tokio::test]
async fn a_cancel_during_the_download_keeps_the_old_data() {
    let big = vec![b'x'; 8 * 1024 * 1024];
    let body = zip_of(&[("BIG-1.json", &big)], true);

    cancelled_update(
        |event| matches!(event, UpdateProgress::Downloading { .. }),
        body,
    )
    .await;
}

#[tokio::test]
async fn a_cancel_during_the_install_keeps_the_old_data() {
    let second = [osv_record(
        "RUSTSEC-0009",
        "crates.io",
        "first-crate",
        "0",
        "9.0.0",
    )];

    cancelled_update(
        |event| matches!(event, UpdateProgress::Installing { .. }),
        osv_zip(&second),
    )
    .await;
}

#[tokio::test]
async fn progress_runs_from_connecting_to_ending_and_ends_each_stage_at_its_total() {
    let dir = tempfile::tempdir().unwrap();
    let records: Vec<Value> = (0..3)
        .map(|n| osv_record(&format!("RUSTSEC-000{n}"), "crates.io", "c", "0", "1.0.0"))
        .collect();
    let archive = osv_zip(&records);
    let total = archive.len() as u64;
    let server = MockServer::start();
    server.route("GET", &archive_path(CARGO), Reply::new(200, archive));
    let (progress, events) = recording();

    updater_at(server.base(), dir.path(), now())
        .update(CARGO, Trigger::Manual, progress, not_cancelled())
        .await;

    let events = events.lock().unwrap().clone();
    assert_eq!(events.first(), Some(&UpdateProgress::Connecting));
    assert_eq!(events.last(), Some(&UpdateProgress::Ending));
    assert_eq!(
        events
            .iter()
            .filter(|event| **event == UpdateProgress::Ending)
            .count(),
        1
    );
    let last_download = events
        .iter()
        .rev()
        .find(|event| matches!(event, UpdateProgress::Downloading { .. }));
    assert_eq!(
        last_download,
        Some(&UpdateProgress::Downloading {
            received: total,
            total
        })
    );
    let installs: Vec<&UpdateProgress> = events
        .iter()
        .filter(|event| matches!(event, UpdateProgress::Installing { .. }))
        .collect();
    assert_eq!(
        (installs.first(), installs.last()),
        (
            Some(&&UpdateProgress::Installing { done: 0, total: 3 }),
            Some(&&UpdateProgress::Installing { done: 3, total: 3 })
        )
    );
}

#[test]
fn the_throttle_lets_one_event_through_every_100_ms() {
    let start = Instant::now();
    let mut throttle = Throttle::default();

    let passed: Vec<bool> = [0, 50, 100, 150, 199, 250]
        .into_iter()
        .map(|ms| throttle.ready(start + Duration::from_millis(ms)))
        .collect();

    assert_eq!(passed, [true, false, true, false, false, true]);
}

#[tokio::test]
async fn an_address_that_cannot_hold_a_path_fails_the_request() {
    let dir = tempfile::tempdir().unwrap();
    let base = Url::parse("data:text/plain,x").unwrap();

    let outcome = update_manually(&updater_at(base, dir.path(), now()), CARGO).await;

    assert!(matches!(
        outcome,
        UpdateOutcome::Failed(UpdateFailure::Request(SourceFailure::Unreachable { .. }))
    ));
}

#[test]
fn the_real_updater_downloads_from_osv_dev_s_bucket() {
    let url = archive_url(&Url::parse(OSV_DATA_BASE).unwrap(), Ecosystem::Cargo).unwrap();

    assert_eq!(
        url.as_str(),
        "https://storage.googleapis.com/osv-vulnerabilities/crates.io/all.zip"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_planted_link_at_the_download_path_is_removed_and_never_written_through() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.txt");
    std::fs::write(&target, b"someone else's file").unwrap();
    std::os::unix::fs::symlink(&target, files(&dir).part).unwrap();

    let outcome = install(dir.path(), CARGO, &first_records(), now()).await;

    assert!(matches!(outcome, UpdateOutcome::Installed { .. }));
    assert_eq!(std::fs::read(&target).unwrap(), b"someone else's file");
}

#[cfg(unix)]
#[tokio::test]
async fn a_store_reached_through_a_link_is_not_opened() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    install(elsewhere.path(), CARGO, &first_records(), now()).await;
    std::os::unix::fs::symlink(StoreFiles::of(elsewhere.path(), CARGO).db, files(&dir).db).unwrap();

    let status = store::status(dir.path(), CARGO);

    assert!(!matches!(status, DataStatus::Ready { .. }), "{status:?}");
}

#[test]
fn each_failure_has_its_kind_and_words() {
    let cases = [
        (
            UpdateFailure::Request(SourceFailure::TimedOut),
            "request",
            "storage.googleapis.com did not answer in time",
        ),
        (
            UpdateFailure::Status(503),
            "status",
            "storage.googleapis.com answered 503",
        ),
        (
            UpdateFailure::NoSize,
            "no_size",
            "the download gave no size",
        ),
        (
            UpdateFailure::TooLarge(9 * 1024 * 1024 * 1024),
            "too_large",
            "the download announced 9.00 GB, more than this tool accepts",
        ),
        (
            UpdateFailure::Deadline,
            "deadline",
            "the daily update took longer than 10 minutes",
        ),
        (
            UpdateFailure::Damaged,
            "damaged",
            "the download was damaged; try again",
        ),
        (
            UpdateFailure::BadArchive("invalid Zip archive".to_owned()),
            "bad_archive",
            "the download is not a readable zip: invalid Zip archive",
        ),
        (
            UpdateFailure::BadRecord {
                id: "GHSA-1".to_owned(),
            },
            "bad_record",
            "the record GHSA-1 could not be read",
        ),
        (
            UpdateFailure::EntryTooLarge {
                id: "GHSA-1".to_owned(),
            },
            "entry_too_large",
            "the record GHSA-1 is larger than 8 MiB",
        ),
        (
            UpdateFailure::NewerFormat,
            "newer_format",
            "the npm data was saved by a newer dep-check",
        ),
        (
            UpdateFailure::Store("disk I/O error".to_owned()),
            "store",
            "the npm data could not be saved: disk I/O error",
        ),
        (
            UpdateFailure::Disk("Access is denied".to_owned()),
            "disk",
            "the npm download could not be written: Access is denied",
        ),
        (
            UpdateFailure::Lock("Access is denied".to_owned()),
            "lock",
            "the update lock could not be taken: Access is denied",
        ),
        (
            UpdateFailure::InUse,
            "in_use",
            "the npm data is in use; try again when the check or removal ends",
        ),
    ];

    for (failure, kind, words) in cases {
        assert_eq!(
            (failure.kind(), failure.describe(Ecosystem::Npm)),
            (kind, words.to_owned())
        );
    }
}

#[tokio::test]
async fn a_closed_port_with_no_data_ends_with_ending_and_no_store() {
    let dir = tempfile::tempdir().unwrap();
    let (progress, events) = recording();

    let outcome = updater_at(closed_port(), dir.path(), now())
        .update(CARGO, Trigger::Manual, progress, not_cancelled())
        .await;

    assert!(matches!(
        outcome,
        UpdateOutcome::Failed(UpdateFailure::Request(SourceFailure::Unreachable { .. }))
    ));
    assert_eq!(store::status(dir.path(), CARGO), DataStatus::NotDownloaded);
    assert_eq!(events.lock().unwrap().last(), Some(&UpdateProgress::Ending));
    assert!(!files(&dir).part.exists());
}
