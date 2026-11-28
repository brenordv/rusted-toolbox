//! Each way an update fails: the old rows stay, `.part` is gone, the attempt is noted on a
//! store that was Ready, and the outcome names the failure.

use super::*;
use crate::test_support::{cut_short, raw_answer};

/// A Ready store of [`first_records`] a day and more old, so an automatic run finds it due,
/// then one update from `base` with `limits`.
async fn fail_after_install(
    base: Url,
    limits: UpdateLimits,
    trigger: Trigger,
) -> (tempfile::TempDir, UpdateOutcome) {
    let dir = tempfile::tempdir().unwrap();
    install(
        dir.path(),
        CARGO,
        &first_records(),
        now() - TimeDelta::hours(30),
    )
    .await;
    let updater = updater_with(base, dir.path(), now(), limits);

    let outcome = updater
        .update(CARGO, trigger, quiet_progress(), not_cancelled())
        .await;

    (dir, outcome)
}

fn quiet_progress() -> ProgressFn {
    Arc::new(|_| {})
}

/// The old rows, no `.part`, and the failure's words noted as the last attempt.
fn assert_kept(dir: &tempfile::TempDir, outcome: &UpdateOutcome) {
    let UpdateOutcome::Failed(failure) = outcome else {
        panic!("not a failure: {outcome:?}");
    };
    assert_eq!(stored_ids(dir.path(), CARGO), ["RUSTSEC-0001"]);
    assert!(!files(dir).part.exists());
    let attempt = meta_of(dir).last_failure.expect("the attempt is noted");
    assert_eq!(attempt.failure, failure.describe(CARGO));
}

async fn served(reply: Reply) -> (MockServer, Url) {
    let server = MockServer::start();
    server.route("GET", &archive_path(CARGO), reply);
    let base = server.base();
    (server, base)
}

#[tokio::test]
async fn a_closed_port_fails_the_request() {
    let (dir, outcome) = fail_after_install(closed_port(), test_limits(), Trigger::Manual).await;

    assert!(matches!(
        outcome,
        UpdateOutcome::Failed(UpdateFailure::Request(SourceFailure::Unreachable { .. }))
    ));
    assert_kept(&dir, &outcome);
}

#[tokio::test]
async fn a_server_that_never_answers_times_out() {
    let (_server, base) = served(Reply::hold()).await;

    let (dir, outcome) = fail_after_install(base, test_limits(), Trigger::Manual).await;

    assert_eq!(
        outcome,
        UpdateOutcome::Failed(UpdateFailure::Request(SourceFailure::TimedOut))
    );
    assert_kept(&dir, &outcome);
}

#[tokio::test]
async fn an_error_status_fails_with_it() {
    let (_server, base) = served(Reply::new(500, "")).await;

    let (dir, outcome) = fail_after_install(base, test_limits(), Trigger::Manual).await;

    assert_eq!(outcome, UpdateOutcome::Failed(UpdateFailure::Status(500)));
    assert_kept(&dir, &outcome);
}

#[tokio::test]
async fn a_download_with_no_size_is_refused() {
    let base = raw_answer(
        b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n5\r\nhello\r\n0\r\n\r\n",
    );

    let (dir, outcome) = fail_after_install(base, test_limits(), Trigger::Manual).await;

    assert_eq!(outcome, UpdateOutcome::Failed(UpdateFailure::NoSize));
    assert_kept(&dir, &outcome);
}

#[tokio::test]
async fn a_download_larger_than_the_limit_is_refused_from_its_headers() {
    let (_server, base) = served(Reply::new(200, vec![0u8; 4096])).await;
    let limits = UpdateLimits {
        max_archive_bytes: 2048,
        ..test_limits()
    };

    let (dir, outcome) = fail_after_install(base, limits, Trigger::Manual).await;

    assert_eq!(
        outcome,
        UpdateOutcome::Failed(UpdateFailure::TooLarge(4096))
    );
    assert_kept(&dir, &outcome);
}

#[tokio::test]
async fn a_body_cut_short_is_interrupted() {
    let base = cut_short(b"PK\x03\x04 only part of a zip");

    let (dir, outcome) = fail_after_install(base, test_limits(), Trigger::Manual).await;

    assert_eq!(
        outcome,
        UpdateOutcome::Failed(UpdateFailure::Request(SourceFailure::Interrupted))
    );
    assert_kept(&dir, &outcome);
}

/// `archive` with the first copy of `content` changed in one byte.
fn tampered(archive: &[u8], content: &[u8]) -> Vec<u8> {
    let start = archive
        .windows(content.len())
        .position(|window| window == content)
        .expect("the content is stored as it is");
    let mut changed = archive.to_vec();
    changed[start] ^= 0x01;
    changed
}

#[tokio::test]
async fn an_entry_whose_bytes_do_not_match_its_checksum_is_damaged() {
    let record = osv_record("RUSTSEC-0009", "crates.io", "first-crate", "0", "9.0.0").to_string();
    let archive = zip_of(&[("RUSTSEC-0009.json", record.as_bytes())], true);
    let (_server, base) = served(Reply::new(200, tampered(&archive, record.as_bytes()))).await;

    let (dir, outcome) = fail_after_install(base, test_limits(), Trigger::Manual).await;

    assert_eq!(outcome, UpdateOutcome::Failed(UpdateFailure::Damaged));
    assert_kept(&dir, &outcome);
}

#[tokio::test]
async fn a_download_that_is_not_a_zip_is_a_bad_archive() {
    let (_server, base) = served(Reply::new(200, "this is not a zip archive")).await;

    let (dir, outcome) = fail_after_install(base, test_limits(), Trigger::Manual).await;

    assert!(matches!(
        outcome,
        UpdateOutcome::Failed(UpdateFailure::BadArchive(_))
    ));
    assert_kept(&dir, &outcome);
}

#[tokio::test]
async fn a_record_without_an_id_fails_the_install_naming_its_entry() {
    let archive = zip_of(&[("MAL-0001.json", br#"{"affected": []}"#)], false);
    let (_server, base) = served(Reply::new(200, archive)).await;

    let (dir, outcome) = fail_after_install(base, test_limits(), Trigger::Manual).await;

    assert_eq!(
        outcome,
        UpdateOutcome::Failed(UpdateFailure::BadRecord {
            id: "MAL-0001".to_owned()
        })
    );
    assert_kept(&dir, &outcome);
}

#[rstest::rstest]
#[case::not_json(b"this is not JSON".as_slice())]
#[case::not_an_object(b"[1, 2, 3]".as_slice())]
#[tokio::test]
async fn a_record_that_is_not_a_json_object_fails_the_install_naming_its_entry(
    #[case] content: &'static [u8],
) {
    let archive = zip_of(&[("MAL-0002.json", content)], false);
    let (_server, base) = served(Reply::new(200, archive)).await;

    let (dir, outcome) = fail_after_install(base, test_limits(), Trigger::Manual).await;

    assert_eq!(
        outcome,
        UpdateOutcome::Failed(UpdateFailure::BadRecord {
            id: "MAL-0002".to_owned()
        })
    );
    assert_kept(&dir, &outcome);
}

#[tokio::test]
async fn a_record_whose_affected_entries_do_not_read_fails_the_install() {
    let archive = zip_of(
        &[(
            "GHSA-0001.json",
            br#"{"id": "GHSA-0001", "affected": [{"versions": "1.0.0"}]}"#,
        )],
        false,
    );
    let (_server, base) = served(Reply::new(200, archive)).await;

    let (dir, outcome) = fail_after_install(base, test_limits(), Trigger::Manual).await;

    assert_eq!(
        outcome,
        UpdateOutcome::Failed(UpdateFailure::BadRecord {
            id: "GHSA-0001".to_owned()
        })
    );
    assert_kept(&dir, &outcome);
}

#[tokio::test]
async fn a_record_larger_than_the_limit_fails_the_install() {
    let big = format!(r#"{{"id": "BIG-1", "summary": "{}"}}"#, "x".repeat(4096));
    let archive = zip_of(&[("BIG-1.json", big.as_bytes())], false);
    let (_server, base) = served(Reply::new(200, archive)).await;
    let limits = UpdateLimits {
        max_entry_bytes: 1024,
        ..test_limits()
    };

    let (dir, outcome) = fail_after_install(base, limits, Trigger::Manual).await;

    assert_eq!(
        outcome,
        UpdateOutcome::Failed(UpdateFailure::EntryTooLarge {
            id: "BIG-1".to_owned()
        })
    );
    assert_kept(&dir, &outcome);
}

/// `archive` (one stored entry of `real` bytes) with both of its headers declaring `declared`
/// bytes instead.
fn understated(archive: &[u8], real: u32, declared: u32) -> Vec<u8> {
    let mut changed = archive.to_vec();
    let size = |offset: usize, archive: &mut Vec<u8>| {
        let field = &mut archive[offset..offset + 4];
        assert_eq!(
            u32::from_le_bytes([field[0], field[1], field[2], field[3]]),
            real
        );
        field.copy_from_slice(&declared.to_le_bytes());
    };
    size(22, &mut changed);
    let central = changed
        .windows(4)
        .position(|window| window == b"PK\x01\x02")
        .expect("a central directory header");
    size(central + 24, &mut changed);
    changed
}

#[tokio::test]
async fn an_entry_larger_than_its_header_says_stops_at_the_declared_size() {
    let record = format!(r#"{{"id": "LIE-1", "summary": "{}"}}"#, "x".repeat(2000));
    let real = u32::try_from(record.len()).unwrap();
    let archive = zip_of(&[("LIE-1.json", record.as_bytes())], true);
    let (_server, base) = served(Reply::new(200, understated(&archive, real, 100))).await;
    let limits = UpdateLimits {
        max_entry_bytes: 1024,
        ..test_limits()
    };

    let (dir, outcome) = fail_after_install(base, limits, Trigger::Manual).await;

    assert_eq!(outcome, UpdateOutcome::Failed(UpdateFailure::Damaged));
    assert_kept(&dir, &outcome);
}

#[tokio::test]
async fn an_automatic_update_past_its_deadline_stops() {
    let (_server, base) = served(Reply::hold()).await;
    let limits = UpdateLimits {
        read_timeout: Duration::from_secs(30),
        auto_deadline: Duration::from_millis(200),
        ..test_limits()
    };

    let (dir, outcome) = fail_after_install(base, limits, Trigger::Auto).await;

    assert_eq!(outcome, UpdateOutcome::Failed(UpdateFailure::Deadline));
    assert_kept(&dir, &outcome);
}

#[tokio::test]
async fn a_download_path_that_cannot_be_created_is_a_disk_failure() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), CARGO, &first_records(), now()).await;
    let part = files(&dir).part;
    std::fs::create_dir(&part).unwrap();
    std::fs::write(part.join("inside"), b"x").unwrap();
    let server = MockServer::start();

    let outcome = update_manually(&updater_at(server.base(), dir.path(), now()), CARGO).await;

    assert!(matches!(
        outcome,
        UpdateOutcome::Failed(UpdateFailure::Disk(_))
    ));
    assert!(server.received().is_empty());
    assert_eq!(stored_ids(dir.path(), CARGO), ["RUSTSEC-0001"]);
    assert!(meta_of(&dir).last_failure.is_some());
}

#[tokio::test]
async fn a_store_that_cannot_take_the_rows_is_a_store_failure() {
    let dir = tempfile::tempdir().unwrap();
    raw(dir.path(), CARGO)
        .execute_batch("CREATE TABLE records (other TEXT);")
        .unwrap();

    let outcome = install(dir.path(), CARGO, &first_records(), now()).await;

    assert!(
        matches!(outcome, UpdateOutcome::Failed(UpdateFailure::Store(_))),
        "{outcome:?}"
    );
    assert_eq!(store::status(dir.path(), CARGO), DataStatus::NotDownloaded);
    assert!(!files(&dir).part.exists());
}

#[tokio::test]
async fn a_folder_where_the_update_lock_should_be_fails_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(store::update_lock(dir.path())).unwrap();
    let server = MockServer::start();

    let outcome = update_manually(&updater_at(server.base(), dir.path(), now()), CARGO).await;

    assert!(
        matches!(outcome, UpdateOutcome::Failed(UpdateFailure::Lock(_))),
        "{outcome:?}"
    );
    assert!(server.received().is_empty());
}

#[tokio::test]
async fn a_download_with_no_record_for_the_ecosystem_is_refused() {
    let npm_only = [osv_record("GHSA-0001", "npm", "left-pad", "0", "1.0.0")];
    let (_server, base) = served(Reply::new(200, osv_zip(&npm_only))).await;

    let (dir, outcome) = fail_after_install(base, test_limits(), Trigger::Manual).await;

    assert_eq!(outcome, UpdateOutcome::Failed(UpdateFailure::NoRecords));
    assert_kept(&dir, &outcome);
}

#[tokio::test]
async fn a_304_to_a_request_that_sent_no_if_none_match_is_a_failure() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), CARGO, &first_records(), now()).await;
    raw(dir.path(), CARGO)
        .execute("UPDATE meta SET etag = 'not\na header'", [])
        .unwrap();
    let (server, base) = served(Reply::new(304, "")).await;

    let outcome = update_manually(&updater_at(base, dir.path(), now()), CARGO).await;

    assert_eq!(outcome, UpdateOutcome::Failed(UpdateFailure::Status(304)));
    assert_eq!(
        server.received_at(&archive_path(CARGO))[0].header("if-none-match"),
        None
    );
}

#[tokio::test]
async fn a_rebuild_whose_install_fails_leaves_a_store_that_still_reads_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), CARGO, &first_records(), now()).await;
    raw(dir.path(), CARGO)
        .execute("UPDATE meta SET format = -1", [])
        .unwrap();
    let bad = zip_of(
        &[("BAD-1.json", br#"{"id": "BAD-1", "affected": 7}"#)],
        false,
    );
    let (_server, base) = served(Reply::new(200, bad)).await;

    let outcome = update_manually(&updater_at(base, dir.path(), now()), CARGO).await;

    assert!(
        matches!(
            outcome,
            UpdateOutcome::Failed(UpdateFailure::BadRecord { .. })
        ),
        "{outcome:?}"
    );
    assert_eq!(
        store::status(dir.path(), CARGO),
        DataStatus::Unreadable {
            reason: "the data is being rebuilt, or a rebuild did not finish".to_owned()
        }
    );
}

#[tokio::test]
async fn a_data_folder_that_cannot_be_created_is_a_disk_failure() {
    let home = tempfile::tempdir().unwrap();
    let blocker = home.path().join("a-file");
    std::fs::write(&blocker, b"x").unwrap();
    let server = MockServer::start();

    let outcome = update_manually(
        &updater_at(server.base(), &blocker.join("offline"), now()),
        CARGO,
    )
    .await;

    assert!(
        matches!(outcome, UpdateOutcome::Failed(UpdateFailure::Disk(_))),
        "{outcome:?}"
    );
    assert!(server.received().is_empty());
}

#[tokio::test]
async fn a_leftover_wal_that_cannot_be_deleted_stops_the_first_install() {
    let dir = tempfile::tempdir().unwrap();
    let wal = files(&dir).wal;
    std::fs::create_dir(&wal).unwrap();
    std::fs::write(wal.join("inside"), b"x").unwrap();
    let (_server, base) = served(Reply::new(200, osv_zip(&first_records()))).await;

    let outcome = update_manually(&updater_at(base, dir.path(), now()), CARGO).await;

    assert!(
        matches!(outcome, UpdateOutcome::Failed(UpdateFailure::Disk(_))),
        "{outcome:?}"
    );
    assert!(!files(&dir).db.exists());
}
