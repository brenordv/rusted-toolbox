//! Offline data inside a whole check: `Fallback` after OSV.dev, `Only` with no service asked.

use super::*;
use crate::offline::locks;
use crate::offline::store::StoreFiles;
use crate::offline::testing::install;
use crate::test_support::osv_record;

fn offline_source(report: &Report) -> &SourceReport {
    &report.sources[3]
}

fn records() -> Vec<serde_json::Value> {
    vec![osv_record(
        "RUSTSEC-0001",
        "crates.io",
        "vuln-crate",
        "0",
        "2.0.0",
    )]
}

async fn installed_at(at: DateTime<Utc>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), Ecosystem::Cargo, &records(), at).await;
    dir
}

fn offline_checker(bases: Bases, dir: &tempfile::TempDir) -> Checker {
    Checker::with_bases(
        TOOL,
        "dep-check-test/1.0",
        bases,
        None,
        Some(dir.path().to_path_buf()),
    )
    .unwrap()
}

fn all_on(server: &MockServer) -> Bases {
    Bases {
        osv: server.base(),
        deps_dev: server.base(),
        github: server.base(),
    }
}

fn all_closed() -> Bases {
    Bases {
        osv: closed_port(),
        deps_dev: closed_port(),
        github: closed_port(),
    }
}

fn offline_request(texts: &[&str], offline: OfflineUse) -> CheckRequest {
    CheckRequest {
        offline,
        ..request(texts)
    }
}

#[tokio::test]
async fn fallback_answers_what_osv_dev_could_not_and_finds_the_advisory() {
    let dir = installed_at(Utc::now()).await;

    let report = offline_checker(all_closed(), &dir)
        .check(
            offline_request(&["vuln-crate@1.0.0"], OfflineUse::Fallback),
            fixed_clock(Utc::now()),
        )
        .await;

    assert_eq!(report.offline, OfflineUse::Fallback);
    assert_eq!(report.sources.len(), 4);
    assert_eq!(offline_source(&report).source, SourceName::Offline);
    let package = &report.packages[0];
    assert_eq!(package.status, PackageStatus::Vulnerable);
    assert_eq!(package.findings[0].sources, [SourceName::Offline]);
    assert_eq!(
        package.answers.get(&SourceName::Offline),
        Some(&AnswerWord::Answered)
    );
}

#[tokio::test]
async fn fallback_answers_the_packages_a_saved_block_kept_osv_dev_from() {
    let dir = installed_at(Utc::now()).await;
    let limits = tempfile::tempdir().unwrap();
    let path = limits_file(&limits);
    let now = Utc::now();
    save_changes(
        Some(&path),
        &[(
            SourceName::Osv,
            Change::Blocked {
                since: now,
                until: now + TimeDelta::minutes(10),
            },
        )],
        now,
    )
    .await;
    let checker = Checker::with_bases(
        TOOL,
        "dep-check-test/1.0",
        all_closed(),
        Some(path),
        Some(dir.path().to_path_buf()),
    )
    .unwrap();

    let report = checker
        .check(
            offline_request(&["other-crate@1.0.0"], OfflineUse::Fallback),
            fixed_clock(now),
        )
        .await;

    assert_eq!(report.packages[0].status, PackageStatus::Clean);
    assert_eq!(
        report.packages[0].answers.get(&SourceName::Offline),
        Some(&AnswerWord::Answered)
    );
}

#[tokio::test]
async fn fallback_does_not_open_the_store_when_osv_dev_answered() {
    let dir = installed_at(Utc::now()).await;
    let _removing =
        locks::try_exclusive(&StoreFiles::of(dir.path(), Ecosystem::Cargo).lock).unwrap();
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, batch(json!([{}])));

    let report = offline_checker(all_on(&server), &dir)
        .check(
            offline_request(&["vuln-crate@1.0.0"], OfflineUse::Fallback),
            fixed_clock(Utc::now()),
        )
        .await;

    assert_eq!(offline_source(&report).status, SourceStatus::NotAsked);
    assert_eq!(
        report.packages[0].answers.get(&SourceName::Offline),
        Some(&AnswerWord::NotAsked)
    );
}

#[tokio::test]
async fn fallback_with_no_data_leaves_offline_data_not_asked() {
    let dir = tempfile::tempdir().unwrap();

    let report = offline_checker(all_closed(), &dir)
        .check(
            offline_request(&["vuln-crate@1.0.0"], OfflineUse::Fallback),
            fixed_clock(Utc::now()),
        )
        .await;

    assert_eq!(offline_source(&report).status, SourceStatus::NotAsked);
    assert_eq!(report.packages[0].status, PackageStatus::NotChecked);
}

#[tokio::test]
async fn off_reports_three_sources_and_no_offline_answers() {
    let dir = installed_at(Utc::now()).await;

    let report = offline_checker(all_closed(), &dir)
        .check(
            offline_request(&["vuln-crate@1.0.0"], OfflineUse::Off),
            fixed_clock(Utc::now()),
        )
        .await;

    assert_eq!(report.sources.len(), 3);
    assert!(
        !report.packages[0]
            .answers
            .contains_key(&SourceName::Offline)
    );
    assert_eq!(report.packages[0].status, PackageStatus::NotChecked);
}

#[tokio::test]
async fn only_asks_no_service_and_needs_a_version() {
    let dir = installed_at(Utc::now()).await;
    let server = MockServer::start();

    let report = offline_checker(all_on(&server), &dir)
        .check(
            offline_request(
                &["vuln-crate@1.0.0", "other-crate@1.0.0", "serde"],
                OfflineUse::Only,
            ),
            fixed_clock(Utc::now()),
        )
        .await;

    assert!(server.received().is_empty());
    let statuses: Vec<PackageStatus> = report.packages.iter().map(|p| p.status).collect();
    assert_eq!(
        statuses,
        [
            PackageStatus::Vulnerable,
            PackageStatus::Clean,
            PackageStatus::NotChecked
        ]
    );
    assert_eq!(report.packages[2].reason.as_deref(), Some(NEEDS_VERSION));
    for source in &report.sources[..3] {
        assert_eq!(source.status, SourceStatus::NotAsked, "{:?}", source.source);
    }
    assert!(offline_source(&report).data_as_of.is_some());
}

#[tokio::test]
async fn only_with_no_data_fails_the_check() {
    let dir = tempfile::tempdir().unwrap();
    let server = MockServer::start();

    let report = offline_checker(all_on(&server), &dir)
        .check(
            offline_request(&["vuln-crate@1.0.0"], OfflineUse::Only),
            fixed_clock(Utc::now()),
        )
        .await;

    assert!(server.received().is_empty());
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.packages[0].reason.as_deref(),
        Some("Offline data is not downloaded for this ecosystem")
    );
}

#[tokio::test]
async fn only_on_a_locked_store_fails_closed_and_sends_no_name() {
    let dir = installed_at(Utc::now()).await;
    let _removing =
        locks::try_exclusive(&StoreFiles::of(dir.path(), Ecosystem::Cargo).lock).unwrap();
    let server = MockServer::start();

    let report = offline_checker(all_on(&server), &dir)
        .check(
            offline_request(&["vuln-crate@1.0.0"], OfflineUse::Only),
            fixed_clock(Utc::now()),
        )
        .await;

    assert!(server.received().is_empty());
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.packages[0].reason.as_deref(),
        Some("Offline data is being removed or rebuilt")
    );
}

#[tokio::test]
async fn only_with_stale_data_and_no_finding_is_not_checked_and_a_finding_still_counts() {
    let dir = installed_at(Utc::now() - TimeDelta::hours(49)).await;

    let report = offline_checker(all_closed(), &dir)
        .check(
            offline_request(&["other-crate@1.0.0", "vuln-crate@1.0.0"], OfflineUse::Only),
            fixed_clock(Utc::now()),
        )
        .await;

    assert_eq!(report.packages[0].status, PackageStatus::NotChecked);
    assert!(
        report.packages[0]
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("too old for a clean result")),
        "{:?}",
        report.packages[0].reason
    );
    assert_eq!(report.packages[1].status, PackageStatus::Vulnerable);
}
