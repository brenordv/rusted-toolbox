//! What offline data writes to the log: one started and one finished line per update, at info
//! unless it failed for a reason other than contention or the user's cancel; the offline
//! source's engine line with `data_as_of`; the settings, removal, and due lines; the blocked
//! checkpoint at debug; and no package name above debug, no address, and no text from a record
//! but a cleaned, cut record id. This is the only test in its binary, so a global subscriber
//! sees every event.

use chrono::{DateTime, TimeDelta, Utc};
use dep_check_core::ecosystem::Ecosystem;
use dep_check_core::engine::{Bases, CheckRequest, Checker};
use dep_check_core::offline::due::{Skip, due};
use dep_check_core::offline::settings::{SettingChange, change, load};
use dep_check_core::offline::store::{remove, status};
use dep_check_core::offline::update::{
    ProgressFn, Trigger, UpdateLimits, UpdateOutcome, UpdateProgress, Updater,
};
use dep_check_core::offline::{DataStatus, OfflineUse};
use dep_check_core::package::parse_specs;
use dep_check_core::test_support::{MockServer, Reply, closed_port, fixed_clock, osv_zip, zip_of};
use serde_json::json;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MARKER: &str = "marker-62342";
const PACKAGE: &str = "zebra-offline-pkg";
const ARCHIVE: &str = "/crates.io/all.zip";
const CARGO: Ecosystem = Ecosystem::Cargo;

#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SharedBuffer {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

fn limits() -> UpdateLimits {
    UpdateLimits {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_millis(500),
        max_archive_bytes: 1024 * 1024,
        max_entry_bytes: 64 * 1024,
        auto_deadline: Duration::from_secs(600),
    }
}

fn updater(server: &MockServer, dir: &Path, now: DateTime<Utc>) -> Updater {
    Updater::with_parts(
        "dep-check-test/1.0",
        server.base(),
        dir.to_path_buf(),
        fixed_clock(now),
        limits(),
    )
    .unwrap()
}

/// Progress that writes nothing but the moment `Ending` arrives, into the same log.
fn marking_progress() -> ProgressFn {
    Arc::new(|event| {
        if event == UpdateProgress::Ending {
            tracing::info!("test saw ending");
        }
    })
}

async fn update(updater: &Updater, trigger: Trigger) -> UpdateOutcome {
    updater
        .update(
            CARGO,
            trigger,
            marking_progress(),
            Arc::new(AtomicBool::new(false)),
        )
        .await
}

fn marked_record() -> serde_json::Value {
    json!({
        "id": "RUSTSEC-0001",
        "aliases": [format!("CVE-{MARKER}")],
        "summary": MARKER,
        "details": MARKER,
        "affected": [{
            "package": {"ecosystem": "crates.io", "name": PACKAGE},
            "ranges": [{"type": "SEMVER", "events": [{"introduced": "0"}, {"fixed": "2.0.0"}]}]
        }]
    })
}

fn check(offline: OfflineUse, texts: &[&str]) -> CheckRequest {
    CheckRequest {
        ecosystem: CARGO,
        packages: parse_specs(CARGO, texts.iter().copied()).unwrap(),
        offline,
    }
}

fn checker(dir: &Path) -> Checker {
    Checker::with_bases(
        "dep-check 1.0.0",
        "dep-check-test/1.0",
        Bases {
            osv: closed_port(),
            deps_dev: closed_port(),
            github: closed_port(),
        },
        None,
        Some(dir.to_path_buf()),
    )
    .unwrap()
}

fn lines_with<'a>(log: &'a str, words: &[&str]) -> Vec<&'a str> {
    log.lines()
        .filter(|line| words.iter().all(|word| line.contains(word)))
        .collect()
}

#[tokio::test]
async fn offline_data_logs_each_update_source_and_change_once_with_no_record_text() {
    let logs = SharedBuffer::default();
    let writer = logs.clone();
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish(),
    )
    .unwrap();
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join("offline");
    let server = MockServer::start();
    server.route(
        "GET",
        ARCHIVE,
        Reply::new(200, osv_zip(&[marked_record()])).with_header("etag", "\"v1\""),
    );
    server.route("GET", ARCHIVE, Reply::new(304, ""));
    server.route("GET", ARCHIVE, Reply::new(500, MARKER));
    let id = format!("\u{1b}[31m{}", "A".repeat(100));
    let bad = format!(
        r#"{{"id": "{}", "affected": "{MARKER}"}}"#,
        id.replace('\u{1b}', "\\u001b")
    );
    server.route(
        "GET",
        ARCHIVE,
        Reply::new(200, zip_of(&[("BAD.json", bad.as_bytes())], false)),
    );
    let now = Utc::now();

    let installed = update(&updater(&server, &dir, now), Trigger::Manual).await;
    let holder = rusqlite::Connection::open(dir.join("cargo.sqlite")).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE;").unwrap();
    let unchanged = update(
        &updater(&server, &dir, now + TimeDelta::days(2)),
        Trigger::Auto,
    )
    .await;
    let failed = update(&updater(&server, &dir, now), Trigger::Manual).await;
    holder.execute_batch("ROLLBACK;").unwrap();
    drop(holder);
    let bad_record = update(&updater(&server, &dir, now), Trigger::Manual).await;
    let busy = {
        let lock = std::fs::File::create(dir.join("update.lock")).unwrap();
        lock.try_lock().unwrap();
        update(&updater(&server, &dir, now), Trigger::Manual).await
    };
    let in_use = {
        let lock = std::fs::File::create(dir.join("cargo.lock")).unwrap();
        lock.try_lock().unwrap();
        update(&updater(&server, &dir, now), Trigger::Manual).await
    };
    let cancelled = updater(&server, &dir, now)
        .update(
            CARGO,
            Trigger::Manual,
            Arc::new(|_| {}),
            Arc::new(AtomicBool::new(true)),
        )
        .await;
    let reader = rusqlite::Connection::open(dir.join("cargo.sqlite")).unwrap();
    reader
        .execute_batch("BEGIN; SELECT count(*) FROM records;")
        .unwrap();
    let checkpoint_server = MockServer::start();
    checkpoint_server.route("GET", ARCHIVE, Reply::new(200, osv_zip(&[marked_record()])));
    let reinstalled = update(&updater(&checkpoint_server, &dir, now), Trigger::Manual).await;
    reader.execute_batch("ROLLBACK;").unwrap();
    drop(reader);
    let skipped = update(&updater(&checkpoint_server, &dir, now), Trigger::Auto).await;

    let only = checker(&dir)
        .check(
            check(OfflineUse::Only, &[&format!("{PACKAGE}@1.0.0")]),
            fixed_clock(now),
        )
        .await;
    let fallback = checker(&dir)
        .check(
            check(OfflineUse::Fallback, &["other-crate@1.0.0"]),
            fixed_clock(now + TimeDelta::days(8)),
        )
        .await;
    let before_off = logs.text().len();
    checker(&dir)
        .check(
            check(OfflineUse::Off, &["other-crate@1.0.0"]),
            fixed_clock(now),
        )
        .await;
    let off_log = logs.text()[before_off..].to_owned();

    let settings = home.path().join("offline.json");
    std::fs::write(&settings, MARKER).unwrap();
    load(Some(&settings));
    change(Some(&settings), SettingChange::AutoUpdate(true)).unwrap();
    let ready = status(&dir, CARGO);
    due(CARGO, &ready, now + TimeDelta::days(2));
    due(Ecosystem::Npm, &DataStatus::NotDownloaded, now);
    remove(&dir, CARGO);
    let log = logs.text();

    assert!(matches!(
        installed,
        UpdateOutcome::Installed { records: 1, .. }
    ));
    assert_eq!(unchanged, UpdateOutcome::Unchanged);
    assert!(matches!(failed, UpdateOutcome::Failed(_)), "{failed:?}");
    assert!(
        matches!(bad_record, UpdateOutcome::Failed(_)),
        "{bad_record:?}"
    );
    assert_eq!(busy, UpdateOutcome::Busy);
    assert!(matches!(in_use, UpdateOutcome::Failed(_)), "{in_use:?}");
    assert!(
        matches!(cancelled, UpdateOutcome::Cancelled { .. }),
        "{cancelled:?}"
    );
    assert!(
        matches!(reinstalled, UpdateOutcome::Installed { .. }),
        "{reinstalled:?}"
    );
    assert_eq!(skipped, UpdateOutcome::Skipped(Skip::NotDue));

    assert_eq!(
        lines_with(&log, &["offline update started"]).len(),
        9,
        "{log}"
    );
    let finished = lines_with(&log, &["offline update finished"]);
    assert_eq!(finished.len(), 9, "{log}");
    let at = |outcome: &str| -> Vec<&&str> {
        finished
            .iter()
            .filter(|line| line.contains(&format!("outcome=\"{outcome}\"")))
            .collect()
    };
    assert!(at("installed").iter().all(|line| line.contains(" INFO ")));
    assert!(at("unchanged")[0].contains("trigger=\"auto\""), "{log}");
    assert!(
        at("unchanged")[0].contains("attempt_write_failed=true"),
        "{log}"
    );
    assert!(at("busy")[0].contains(" INFO "), "{log}");
    assert!(at("cancelled")[0].contains(" INFO "), "{log}");
    let skipped_line = at("skipped")[0];
    assert!(
        skipped_line.contains(" INFO ")
            && skipped_line.contains("reason=\"not_due\"")
            && skipped_line.contains("from=\"ready\""),
        "{skipped_line}"
    );
    let failures = at("failed");
    assert_eq!(failures.len(), 3, "{log}");
    let in_use_line = failures
        .iter()
        .find(|line| line.contains("kind=\"in_use\""))
        .expect("the in-use line");
    assert!(in_use_line.contains(" INFO "), "{in_use_line}");
    let status_line = failures
        .iter()
        .find(|line| line.contains("kind=\"status\""))
        .expect("the 500 line");
    assert!(status_line.contains(" WARN "), "{status_line}");
    assert!(
        status_line.contains("attempt_write_failed=true"),
        "{status_line}"
    );
    let bad_line = failures
        .iter()
        .find(|line| line.contains("kind=\"bad_record\""))
        .expect("the bad record line");
    assert!(bad_line.contains(" WARN "), "{bad_line}");
    assert!(!bad_line.contains('\u{1b}'), "{bad_line}");
    let shown_id = format!(" [31m{}", "A".repeat(59));
    assert!(
        bad_line.contains(&format!("the record {shown_id} could not be read")),
        "{bad_line}"
    );
    assert!(!bad_line.contains(&"A".repeat(60)), "{bad_line}");

    let ending = log.find("test saw ending").expect("an ending");
    let first_finished = log
        .find("offline update finished")
        .expect("a finished line");
    assert!(ending < first_finished, "{log}");
    assert_eq!(
        lines_with(
            &log,
            &[
                "DEBUG",
                "the checkpoint after the install was blocked by a reader"
            ]
        )
        .len(),
        1,
        "{log}"
    );

    assert_eq!(only.packages[0].findings.len(), 1);
    let offline_lines = lines_with(&log, &["source finished", "source=\"offline\""]);
    assert_eq!(offline_lines.len(), 2, "{log}");
    assert!(
        offline_lines[0].contains(" INFO ") && offline_lines[0].contains("data_as_of=\"2"),
        "{log}"
    );
    assert!(
        offline_lines[1].contains(" WARN ")
            && offline_lines[1].contains("too old for a clean result"),
        "{log}"
    );
    assert!(offline_lines[1].contains("data_as_of=\"2"), "{log}");
    assert_eq!(fallback.offline, OfflineUse::Fallback);
    assert_eq!(
        lines_with(&log, &["check started", "offline=\"only\""]).len(),
        1,
        "{log}"
    );
    assert_eq!(
        lines_with(&log, &["check started", "offline=\"fallback\""]).len(),
        1,
        "{log}"
    );
    assert_eq!(
        lines_with(&log, &["check started", "offline=\"off\""]).len(),
        1,
        "{log}"
    );
    assert_eq!(
        lines_with(&off_log, &["source finished"]).len(),
        3,
        "{off_log}"
    );
    assert!(
        lines_with(&off_log, &["source=\"offline\""]).is_empty(),
        "{off_log}"
    );

    assert_eq!(
        lines_with(&log, &[" WARN ", "the offline settings could not be read"]).len(),
        1,
        "{log}"
    );
    assert_eq!(
        lines_with(&log, &["offline settings changed", "auto_update=true"]).len(),
        1,
        "{log}"
    );
    assert_eq!(
        lines_with(&log, &["offline data removed", "ecosystem=\"cargo\""]).len(),
        1,
        "{log}"
    );
    assert_eq!(
        lines_with(&log, &["DEBUG", "offline update due", "verdict=\"due\""]).len(),
        1,
        "{log}"
    );
    assert_eq!(
        lines_with(
            &log,
            &[
                "DEBUG",
                "offline update due",
                "verdict=\"skipped\"",
                "reason=\"not_downloaded\""
            ]
        )
        .len(),
        1,
        "{log}"
    );

    let port = format!(":{}", server.addr.port());
    for line in log.lines().filter(|line| line.contains("dep_check_core")) {
        assert!(!line.contains(MARKER), "record or answer text in: {line}");
        if line.contains("DEBUG") || line.contains("TRACE") {
            continue;
        }
        assert!(!line.contains(PACKAGE), "a package name in: {line}");
        assert!(!line.contains("other-crate"), "a package name in: {line}");
        assert!(!line.contains(&port), "the address in: {line}");
        assert!(!line.contains("all.zip"), "the download path in: {line}");
    }
}
