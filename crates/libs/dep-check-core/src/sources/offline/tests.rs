use super::*;
use crate::offline::locks;
use crate::offline::store::StoreFiles;
use crate::offline::testing::{archive_path, install, raw, update_manually, updater_at};
use crate::test_support::{MockServer, Reply, osv_record, osv_zip};
use serde_json::json;

const CARGO: Ecosystem = Ecosystem::Cargo;

fn package(name: &str, version: &str) -> Resolved {
    Resolved {
        key: PackageKey(0),
        name: name.to_owned(),
        version: version.to_owned(),
    }
}

fn records() -> Vec<Value> {
    vec![osv_record(
        "RUSTSEC-0001",
        "crates.io",
        "Vuln_Crate",
        "0",
        "2.0.0",
    )]
}

fn run_of(read: OfflineRead) -> SourceRun {
    match read {
        OfflineRead::Run(run) => run,
        OfflineRead::NoData => panic!("no data"),
    }
}

fn answer_for(dir: &tempfile::TempDir, package: Resolved, max_age: TimeDelta) -> PackageAnswer {
    let source = OfflineSource::new(Some(dir.path().to_path_buf()));
    let mut run = run_of(source.check(CARGO, &[package], max_age, Utc::now()));
    run.answers.remove(&PackageKey(0)).expect("an answer")
}

async fn installed_at(at: DateTime<Utc>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), CARGO, &records(), at).await;
    dir
}

#[tokio::test]
async fn an_affected_version_is_answered_with_the_advisory_under_any_spelling() {
    let dir = installed_at(Utc::now()).await;

    let answer = answer_for(&dir, package("vuln-crate", "1.0.0"), TimeDelta::days(7));

    let PackageAnswer::Answered { advisories } = answer else {
        panic!("not answered: {answer:?}");
    };
    assert_eq!(advisories.len(), 1);
    assert_eq!(
        (
            advisories[0].id.as_str(),
            advisories[0].source,
            advisories[0].fixed_in.clone()
        ),
        (
            "RUSTSEC-0001",
            SourceName::Offline,
            vec!["2.0.0".to_owned()]
        )
    );
}

#[tokio::test]
async fn a_fixed_version_on_fresh_data_is_answered_clean() {
    let dir = installed_at(Utc::now() - TimeDelta::days(6)).await;

    let answer = answer_for(&dir, package("vuln-crate", "2.0.0"), TimeDelta::days(7));

    assert_eq!(
        answer,
        PackageAnswer::Answered {
            advisories: Vec::new()
        }
    );
}

#[tokio::test]
async fn no_finding_on_data_older_than_the_window_is_stale() {
    let at = Utc::now() - TimeDelta::days(8);
    let dir = installed_at(at).await;

    let answer = answer_for(&dir, package("other-crate", "1.0.0"), TimeDelta::days(7));

    assert_eq!(
        answer.failure(),
        Some(&SourceFailure::OfflineStale { as_of: at })
    );
}

#[tokio::test]
async fn data_49_hours_old_is_stale_offline_only() {
    let dir = installed_at(Utc::now() - TimeDelta::hours(49)).await;

    let answer = answer_for(&dir, package("other-crate", "1.0.0"), TimeDelta::hours(48));

    assert!(matches!(
        answer.failure(),
        Some(SourceFailure::OfflineStale { .. })
    ));
}

#[tokio::test]
async fn a_finding_counts_however_old_the_data_is() {
    let dir = installed_at(Utc::now() - TimeDelta::days(30)).await;

    let answer = answer_for(&dir, package("vuln-crate", "1.0.0"), TimeDelta::days(7));

    assert!(matches!(answer, PackageAnswer::Answered { ref advisories } if advisories.len() == 1));
}

#[tokio::test]
async fn data_from_the_future_is_stale() {
    let dir = installed_at(Utc::now() + TimeDelta::hours(1)).await;

    let answer = answer_for(&dir, package("other-crate", "1.0.0"), TimeDelta::days(7));

    assert!(matches!(
        answer.failure(),
        Some(SourceFailure::OfflineStale { .. })
    ));
}

#[tokio::test]
async fn old_data_confirmed_current_today_answers_clean() {
    let dir = tempfile::tempdir().unwrap();
    let server = MockServer::start();
    let path = archive_path(CARGO);
    server.route(
        "GET",
        &path,
        Reply::new(200, osv_zip(&records())).with_header("etag", "\"v1\""),
    );
    server.route("GET", &path, Reply::new(304, ""));
    let base = server.base();
    update_manually(
        &updater_at(base.clone(), dir.path(), Utc::now() - TimeDelta::days(8)),
        CARGO,
    )
    .await;
    update_manually(&updater_at(base, dir.path(), Utc::now()), CARGO).await;

    let answer = answer_for(&dir, package("other-crate", "1.0.0"), TimeDelta::days(7));

    assert_eq!(
        answer,
        PackageAnswer::Answered {
            advisories: Vec::new()
        }
    );
}

#[tokio::test]
async fn a_record_that_cannot_be_decided_fails_the_package() {
    let dir = tempfile::tempdir().unwrap();
    let undecidable = json!({
        "id": "GHSA-0002",
        "affected": [{"package": {"ecosystem": "crates.io", "name": "odd"},
                      "ranges": [{"type": "GIT", "events": [{"introduced": "0"}]}]}]
    });
    install(dir.path(), CARGO, &[undecidable], Utc::now()).await;

    let answer = answer_for(&dir, package("odd", "1.0.0"), TimeDelta::days(7));

    assert_eq!(answer.failure(), Some(&SourceFailure::OfflineUndecided));
}

#[tokio::test]
async fn an_index_row_with_no_record_fails_the_package() {
    let dir = installed_at(Utc::now()).await;
    raw(dir.path(), CARGO)
        .execute("DELETE FROM records", [])
        .unwrap();

    let answer = answer_for(&dir, package("vuln-crate", "3.0.0"), TimeDelta::days(7));

    assert_eq!(answer.failure(), Some(&SourceFailure::OfflineUndecided));
}

#[tokio::test]
async fn a_record_whose_severity_has_no_score_keeps_the_finding_without_cvss() {
    let dir = tempfile::tempdir().unwrap();
    let mut record = osv_record("RUSTSEC-0003", "crates.io", "crate", "0", "2.0.0");
    record["severity"] = json!([{"type": "CVSS_V3"}]);
    install(dir.path(), CARGO, &[record], Utc::now()).await;
    let source = OfflineSource::new(Some(dir.path().to_path_buf()));

    let run = run_of(source.check(
        CARGO,
        &[package("crate", "1.0.0")],
        TimeDelta::days(7),
        Utc::now(),
    ));

    let advisories = run.answers[&PackageKey(0)].advisories();
    assert_eq!(advisories[0].id, "RUSTSEC-0003");
    assert!(advisories[0].cvss.is_empty());
    assert_eq!(
        advisories[0].summary.as_deref(),
        Some("summary of RUSTSEC-0003")
    );
    assert_eq!(run.details_missing, 0);
}

#[tokio::test]
async fn a_record_whose_details_do_not_read_is_kept_as_its_id() {
    let dir = tempfile::tempdir().unwrap();
    let mut record = osv_record("RUSTSEC-0004", "crates.io", "crate", "0", "2.0.0");
    record["published"] = json!("not a date");
    install(dir.path(), CARGO, &[record], Utc::now()).await;
    let source = OfflineSource::new(Some(dir.path().to_path_buf()));

    let run = run_of(source.check(
        CARGO,
        &[package("crate", "1.0.0")],
        TimeDelta::days(7),
        Utc::now(),
    ));

    let advisories = run.answers[&PackageKey(0)].advisories();
    assert_eq!(advisories[0].id, "RUSTSEC-0004");
    assert!(advisories[0].summary.is_none());
    assert_eq!(run.details_missing, 1);
}

#[tokio::test]
async fn the_run_carries_when_the_data_was_confirmed_current() {
    let at = Utc::now() - TimeDelta::hours(3);
    let dir = installed_at(at).await;
    let source = OfflineSource::new(Some(dir.path().to_path_buf()));

    let run = run_of(source.check(
        CARGO,
        &[package("crate", "1.0.0")],
        TimeDelta::days(7),
        Utc::now(),
    ));

    assert_eq!(
        (run.source, run.data_as_of),
        (SourceName::Offline, Some(at))
    );
}

#[test]
fn no_store_and_no_home_folder_have_no_data() {
    let dir = tempfile::tempdir().unwrap();

    let empty = OfflineSource::new(Some(dir.path().to_path_buf()));
    let homeless = OfflineSource::new(None);

    for source in [empty, homeless] {
        assert_eq!(
            source.check(
                CARGO,
                &[package("crate", "1.0.0")],
                TimeDelta::days(7),
                Utc::now()
            ),
            OfflineRead::NoData
        );
    }
}

#[tokio::test]
async fn a_store_a_removal_holds_fails_every_package_as_busy() {
    let dir = installed_at(Utc::now()).await;
    let _removing = locks::try_exclusive(&StoreFiles::of(dir.path(), CARGO).lock).unwrap();

    let answer = answer_for(&dir, package("crate", "1.0.0"), TimeDelta::days(7));

    assert_eq!(answer.failure(), Some(&SourceFailure::OfflineBusy));
}

#[tokio::test]
async fn an_unreadable_store_fails_every_package_with_its_reason() {
    let dir = installed_at(Utc::now()).await;
    raw(dir.path(), CARGO)
        .execute("UPDATE meta SET format = -1", [])
        .unwrap();

    let answer = answer_for(&dir, package("crate", "1.0.0"), TimeDelta::days(7));

    assert_eq!(
        answer.failure(),
        Some(&SourceFailure::OfflineUnreadable {
            reason: "the data has format -1".to_owned()
        })
    );
}

#[tokio::test]
async fn a_newer_store_fails_every_package_naming_the_newer_tool() {
    let dir = installed_at(Utc::now()).await;
    raw(dir.path(), CARGO)
        .execute("UPDATE meta SET format = 2", [])
        .unwrap();

    let answer = answer_for(&dir, package("crate", "1.0.0"), TimeDelta::days(7));

    assert_eq!(
        answer.failure(),
        Some(&SourceFailure::OfflineUnreadable {
            reason: NEWER_FORMAT.to_owned()
        })
    );
}

#[tokio::test]
async fn a_busy_store_fails_every_package_as_unreadable_for_now() {
    let dir = installed_at(Utc::now()).await;
    let holder = raw(dir.path(), CARGO);
    holder
        .execute_batch(
            "PRAGMA locking_mode = EXCLUSIVE; BEGIN IMMEDIATE; UPDATE meta SET records = records;",
        )
        .unwrap();

    let answer = answer_for(&dir, package("crate", "1.0.0"), TimeDelta::days(7));

    assert!(matches!(
        answer.failure(),
        Some(SourceFailure::OfflineUnreadable { .. })
    ));
    holder.execute_batch("ROLLBACK").unwrap();
}

#[tokio::test]
async fn a_read_task_that_dies_fails_every_package_as_stopped() {
    let keys = [PackageKey(0), PackageKey(3)];
    let died = tokio::task::spawn_blocking(|| -> OfflineRead { panic!("the read broke") }).await;

    let run = run_of(joined(died, &keys));

    assert_eq!(run.answers.len(), 2);
    assert!(
        run.answers
            .values()
            .all(|answer| answer.failure() == Some(&SourceFailure::OfflineStopped))
    );
}
