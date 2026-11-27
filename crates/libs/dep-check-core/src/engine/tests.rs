use super::*;
use crate::advisory::{AdvisoryKind, Severity};
use crate::rate_limits::{Block, Change, load, save_changes};
use crate::report::{BlockOrigin, FailureGroup, SourceReport, SourceStatus};
use crate::test_support::{MockServer, Reply, closed_port, fixed_clock};
use chrono::TimeDelta;
use serde_json::json;

const TOOL: &str = "dep-check 1.0.0";
const QUERYBATCH: &str = "/v1/querybatch";

mod github;
mod offline;

/// OSV.dev at `osv_base`, deps.dev on a port nothing listens on.
fn checker(osv_base: Url, limits_path: Option<PathBuf>) -> Checker {
    checker_with(osv_base, closed_port(), limits_path)
}

/// OSV.dev and deps.dev at the given bases, GitHub on a port nothing listens on.
fn checker_with(osv_base: Url, deps_dev_base: Url, limits_path: Option<PathBuf>) -> Checker {
    checker_of(
        Bases {
            osv: osv_base,
            deps_dev: deps_dev_base,
            github: closed_port(),
        },
        limits_path,
    )
}

fn checker_of(bases: Bases, limits_path: Option<PathBuf>) -> Checker {
    Checker::with_bases(TOOL, "dep-check-test/1.0", bases, limits_path, None).unwrap()
}

fn osv_source(report: &Report) -> &SourceReport {
    &report.sources[0]
}

fn deps_dev_source(report: &Report) -> &SourceReport {
    &report.sources[1]
}

fn spec(text: &str) -> PackageSpec {
    crate::package::parse_spec(Ecosystem::Cargo, text).unwrap()
}

fn request(texts: &[&str]) -> CheckRequest {
    CheckRequest {
        ecosystem: Ecosystem::Cargo,
        packages: texts.iter().map(|text| spec(text)).collect(),
        offline: OfflineUse::Off,
    }
}

fn batch(results: serde_json::Value) -> Reply {
    Reply::json(200, &json!({ "results": results }))
}

fn limits_file(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("dep-check").join("rate-limits.json")
}

#[tokio::test]
async fn a_full_run_reports_findings_counts_and_the_clock_s_time() {
    let server = MockServer::start();
    server.route(
        "POST",
        QUERYBATCH,
        batch(json!([{"vulns": [{"id": "RUSTSEC-2023-0005"}]}, {}])),
    );
    server.route(
        "GET",
        "/v1/vulns/RUSTSEC-2023-0005",
        Reply::json(
            200,
            &json!({"id": "RUSTSEC-2023-0005", "aliases": ["CVE-2023-22466"]}),
        ),
    );
    let now = Utc::now();

    let report = checker(server.base(), None)
        .check(
            request(&["tokio@1.20.0", "serde@1.0.215"]),
            fixed_clock(now),
        )
        .await;

    assert_eq!(report.outcome, Outcome::Vulnerable);
    assert_eq!(report.checked_at, now);
    assert_eq!(report.tool, TOOL);
    assert_eq!(
        report.summary,
        Summary {
            packages: 2,
            vulnerable: 1,
            clean: 1,
            not_found: 0,
            not_checked: 0,
            findings: 1
        }
    );
    assert_eq!(report.packages[0].findings[0].aliases, ["CVE-2023-22466"]);
    assert_eq!(
        report.sources[0].status,
        SourceStatus::Answered {
            blocked_until: None
        }
    );
    assert_eq!(
        report.packages[1].answers[&SourceName::Osv],
        AnswerWord::Answered
    );
}

#[tokio::test]
async fn a_saved_block_keeps_osv_from_being_asked() {
    let server = MockServer::start();
    let dir = tempfile::tempdir().unwrap();
    let path = limits_file(&dir);
    let now = Utc::now();
    let until = now + TimeDelta::minutes(10);
    save_changes(
        Some(&path),
        &[(SourceName::Osv, Change::Blocked { since: now, until })],
        now,
    )
    .await;

    let report = checker(server.base(), Some(path))
        .check(request(&["tokio@1.20.0"]), fixed_clock(now))
        .await;

    assert!(server.received().is_empty());
    assert_eq!(
        report.sources[0].status,
        SourceStatus::RateLimited {
            until,
            origin: BlockOrigin::Saved
        }
    );
    assert_eq!(report.sources[0].failures, Vec::new());
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.packages[0].answers[&SourceName::Osv],
        AnswerWord::RateLimited
    );
}

#[tokio::test]
async fn a_block_during_the_run_is_on_disk_after_it_with_one_strike() {
    let server = MockServer::start();
    let dir = tempfile::tempdir().unwrap();
    let path = limits_file(&dir);
    server.route("POST", QUERYBATCH, batch(json!(vec![json!({}); 1000])));
    server.route(
        "POST",
        QUERYBATCH,
        Reply::new(429, "").with_header("retry-after", "90"),
    );
    let now = Utc::now();
    let texts: Vec<String> = (0..1001)
        .map(|index| format!("crate{index}@1.0.0"))
        .collect();
    let text_refs: Vec<&str> = texts.iter().map(String::as_str).collect();

    let report = checker(server.base(), Some(path.clone()))
        .check(request(&text_refs), fixed_clock(now))
        .await;

    let saved = load(Some(&path), now).await;
    assert_eq!(
        saved.entry(SourceName::Osv),
        Some(Block {
            since: now,
            until: now + TimeDelta::seconds(90),
            strikes: 1
        })
    );
    assert_eq!(
        report.sources[0].status,
        SourceStatus::Partial {
            blocked_until: Some(now + TimeDelta::seconds(90))
        }
    );
}

#[tokio::test]
async fn an_answer_after_an_earlier_block_clears_the_block_it_loaded() {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, batch(json!([{}])));
    let dir = tempfile::tempdir().unwrap();
    let path = limits_file(&dir);
    let now = Utc::now();
    save_changes(
        Some(&path),
        &[(
            SourceName::Osv,
            Change::Blocked {
                since: now - TimeDelta::minutes(10),
                until: now - TimeDelta::minutes(5),
            },
        )],
        now,
    )
    .await;

    let report = checker(server.base(), Some(path.clone()))
        .check(request(&["tokio@1.20.0"]), fixed_clock(now))
        .await;

    assert_eq!(report.outcome, Outcome::Clean);
    assert_eq!(load(Some(&path), now).await.entry(SourceName::Osv), None);
}

#[tokio::test]
async fn osv_down_for_every_package_fails_the_check() {
    let report = checker(closed_port(), None)
        .check(
            request(&["tokio@1.20.0", "serde@1.0.215"]),
            fixed_clock(Utc::now()),
        )
        .await;

    assert_eq!(report.outcome, Outcome::Failed);
    assert!(matches!(
        report.sources[0].status,
        SourceStatus::Failed {
            failure: SourceFailure::Unreachable { .. }
        }
    ));
    assert_eq!(report.sources[0].failures[0].packages, 2);
    assert!(
        report.packages[0]
            .reason
            .as_deref()
            .unwrap()
            .starts_with("OSV.dev could not be reached: ")
    );
}

#[tokio::test]
async fn a_run_that_answers_nothing_keeps_a_lingering_entry_and_its_strikes() {
    let dir = tempfile::tempdir().unwrap();
    let path = limits_file(&dir);
    let now = Utc::now();
    let lingering = Change::Blocked {
        since: now - TimeDelta::minutes(10),
        until: now - TimeDelta::minutes(5),
    };
    save_changes(Some(&path), &[(SourceName::Osv, lingering.clone())], now).await;
    save_changes(Some(&path), &[(SourceName::Osv, lingering)], now).await;
    let before = load(Some(&path), now).await.entry(SourceName::Osv);

    let report = checker(closed_port(), Some(path.clone()))
        .check(request(&["tokio@1.20.0"]), fixed_clock(now))
        .await;

    assert_eq!(report.outcome, Outcome::Failed);
    let after = load(Some(&path), now).await.entry(SourceName::Osv);
    assert_eq!(after, before);
    assert_eq!(after.map(|block| block.strikes), Some(2));
}

#[tokio::test]
async fn a_saved_block_with_no_versioned_package_reads_not_needed() {
    let server = MockServer::start();
    let dir = tempfile::tempdir().unwrap();
    let path = limits_file(&dir);
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

    let report = checker(server.base(), Some(path))
        .check(request(&["serde"]), fixed_clock(now))
        .await;

    assert_eq!(report.sources[0].status, SourceStatus::NotAsked);
    assert!(server.received().is_empty());
}

#[tokio::test]
async fn a_rate_limit_file_under_a_file_does_not_fail_the_check() {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, batch(json!([{}])));
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("dep-check"), b"a file").unwrap();

    let report = checker(server.base(), Some(limits_file(&dir)))
        .check(request(&["tokio@1.20.0"]), fixed_clock(Utc::now()))
        .await;

    assert_eq!(report.outcome, Outcome::Clean);
}

const SERDE_PACKAGE: &str = "/v3/systems/cargo/packages/serde";
const SERDE_VERSION: &str = "/v3/systems/cargo/packages/serde/versions/1.0.215";

fn serde_versions() -> Reply {
    Reply::json(
        200,
        &json!({"versions": [
            {"versionKey": {"version": "1.0.214"}, "isDefault": false},
            {"versionKey": {"version": "1.0.215"}, "isDefault": true}
        ]}),
    )
}

#[tokio::test]
async fn a_package_without_a_version_is_checked_at_deps_dev_s_latest() {
    let server = MockServer::start();
    server.route("GET", SERDE_PACKAGE, serde_versions());
    server.route(
        "GET",
        SERDE_VERSION,
        Reply::json(
            200,
            &json!({"isDefault": true, "licenses": ["MIT OR Apache-2.0"]}),
        ),
    );
    server.route("POST", QUERYBATCH, batch(json!([{}])));

    let report = checker_with(server.base(), server.base(), None)
        .check(request(&["serde"]), fixed_clock(Utc::now()))
        .await;

    let package = &report.packages[0];
    assert_eq!(
        (package.version.as_deref(), package.version_from),
        (Some("1.0.215"), Some(VersionFrom::DepsDevLatest))
    );
    assert_eq!(package.status, PackageStatus::Clean);
    assert_eq!(
        package.supply_chain.as_ref().map(|chain| chain.latest),
        Some(true)
    );
    let query = &server.received_at(QUERYBATCH)[0].json()["queries"][0];
    assert_eq!(query["version"], "1.0.215");
    assert_eq!(deps_dev_source(&report).requests, 2);
}

#[tokio::test]
async fn osv_down_and_deps_dev_up_with_an_advisory_is_vulnerable() {
    let server = MockServer::start();
    server.route(
        "GET",
        SERDE_VERSION,
        Reply::json(200, &json!({"advisoryKeys": [{"id": "GHSA-1"}]})),
    );
    server.route(
        "GET",
        "/v3/advisories/GHSA-1",
        Reply::json(200, &json!({"title": "bad", "cvss3Score": 7.5})),
    );

    let report = checker_with(closed_port(), server.base(), None)
        .check(request(&["serde@1.0.215"]), fixed_clock(Utc::now()))
        .await;

    assert_eq!(report.packages[0].status, PackageStatus::Vulnerable);
    assert_eq!(report.packages[0].findings[0].severity, Severity::High);
    assert!(matches!(
        osv_source(&report).status,
        SourceStatus::Failed { .. }
    ));
}

#[tokio::test]
async fn deps_dev_down_and_osv_up_is_decided_by_osv_without_a_supply_chain() {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, batch(json!([{}])));

    let report = checker_with(server.base(), closed_port(), None)
        .check(request(&["serde@1.0.215"]), fixed_clock(Utc::now()))
        .await;

    assert_eq!(report.packages[0].status, PackageStatus::Clean);
    assert_eq!(report.packages[0].supply_chain, None);
    assert!(matches!(
        deps_dev_source(&report).status,
        SourceStatus::Failed { .. }
    ));
}

#[tokio::test]
async fn one_advisory_from_both_sources_is_one_finding_with_deps_dev_s_score() {
    let server = MockServer::start();
    server.route(
        "POST",
        QUERYBATCH,
        batch(json!([{"vulns": [{"id": "RUSTSEC-1"}]}])),
    );
    server.route(
        "GET",
        "/v1/vulns/RUSTSEC-1",
        Reply::json(200, &json!({"id": "RUSTSEC-1", "aliases": ["GHSA-1"]})),
    );
    server.route(
        "GET",
        SERDE_VERSION,
        Reply::json(200, &json!({"advisoryKeys": [{"id": "GHSA-1"}]})),
    );
    server.route(
        "GET",
        "/v3/advisories/GHSA-1",
        Reply::json(200, &json!({"aliases": ["CVE-1"], "cvss3Score": 9.8})),
    );

    let report = checker_with(server.base(), server.base(), None)
        .check(request(&["serde@1.0.215"]), fixed_clock(Utc::now()))
        .await;

    let findings = &report.packages[0].findings;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].id, "RUSTSEC-1");
    assert_eq!(findings[0].aliases, ["CVE-1", "GHSA-1"]);
    assert_eq!(findings[0].sources, [SourceName::Osv, SourceName::DepsDev]);
    assert_eq!(findings[0].severity, Severity::Critical);
}

#[tokio::test]
async fn a_version_deps_dev_does_not_know_with_a_mal_advisory_is_vulnerable_malware() {
    let server = MockServer::start();
    server.route(
        "POST",
        QUERYBATCH,
        batch(json!([{"vulns": [{"id": "MAL-2026-1"}]}])),
    );
    server.route(
        "GET",
        "/v1/vulns/MAL-2026-1",
        Reply::json(200, &json!({"id": "MAL-2026-1"})),
    );
    server.route("GET", SERDE_VERSION, Reply::new(404, ""));

    let report = checker_with(server.base(), server.base(), None)
        .check(request(&["serde@1.0.215"]), fixed_clock(Utc::now()))
        .await;

    assert_eq!(report.packages[0].status, PackageStatus::Vulnerable);
    assert_eq!(report.packages[0].findings[0].kind, AdvisoryKind::Malware);
    assert_eq!(
        report.packages[0].answers[&SourceName::DepsDev],
        AnswerWord::NotFound
    );
}

#[tokio::test]
async fn a_lookup_answer_alone_clears_the_deps_dev_entry_it_loaded() {
    let server = MockServer::start();
    server.route(
        "GET",
        "/v3/systems/cargo/packages/left-padd",
        Reply::new(404, ""),
    );
    let dir = tempfile::tempdir().unwrap();
    let path = limits_file(&dir);
    let now = Utc::now();
    save_changes(
        Some(&path),
        &[(
            SourceName::DepsDev,
            Change::Blocked {
                since: now - TimeDelta::minutes(10),
                until: now - TimeDelta::minutes(5),
            },
        )],
        now,
    )
    .await;

    checker_with(server.base(), server.base(), Some(path.clone()))
        .check(request(&["left-padd"]), fixed_clock(now))
        .await;

    assert_eq!(
        load(Some(&path), now).await.entry(SourceName::DepsDev),
        None
    );
}

#[rstest::rstest]
#[case::answered(Some(PackageAnswer::Answered { advisories: Vec::new() }))]
#[case::not_asked(None)]
fn a_package_left_without_a_version_by_any_other_answer_is_not_checked(
    #[case] lookup: Option<PackageAnswer>,
) {
    let verdict = without_version(lookup.as_ref(), OfflineUse::Off);

    assert_eq!(
        verdict,
        (
            PackageStatus::NotChecked,
            Some("no version was given, and deps.dev did not name the latest one".to_owned())
        )
    );
}

#[test]
fn a_package_left_without_a_version_offline_only_needs_one() {
    let verdict = without_version(None, OfflineUse::Only);

    assert_eq!(
        verdict,
        (PackageStatus::NotChecked, Some(NEEDS_VERSION.to_owned()))
    );
}

#[tokio::test]
async fn a_package_without_a_version_that_deps_dev_does_not_know_is_not_found() {
    let server = MockServer::start();
    server.route(
        "GET",
        "/v3/systems/cargo/packages/left-padd",
        Reply::new(404, ""),
    );

    let report = checker_with(server.base(), server.base(), None)
        .check(request(&["left-padd"]), fixed_clock(Utc::now()))
        .await;

    let package = &report.packages[0];
    assert_eq!(
        (package.status, package.version.as_deref()),
        (PackageStatus::NotFound, None)
    );
    assert_eq!(package.answers[&SourceName::Osv], AnswerWord::NotAsked);
    assert_eq!(package.answers[&SourceName::DepsDev], AnswerWord::NotFound);
    assert!(server.received_at(QUERYBATCH).is_empty());
    assert_eq!(
        deps_dev_source(&report).status,
        SourceStatus::Answered {
            blocked_until: None
        }
    );
    assert_eq!(report.outcome, Outcome::Incomplete);
}

#[tokio::test]
async fn a_package_without_a_version_while_deps_dev_is_blocked_names_the_block() {
    let server = MockServer::start();
    let dir = tempfile::tempdir().unwrap();
    let path = limits_file(&dir);
    let now = Utc::now();
    let until = now + TimeDelta::minutes(10);
    save_changes(
        Some(&path),
        &[(SourceName::DepsDev, Change::Blocked { since: now, until })],
        now,
    )
    .await;

    let report = checker_with(server.base(), server.base(), Some(path))
        .check(request(&["serde"]), fixed_clock(now))
        .await;

    assert!(server.received().is_empty());
    assert_eq!(report.packages[0].status, PackageStatus::NotChecked);
    assert_eq!(
        report.packages[0].reason,
        Some(format!(
            "no version was given, and deps.dev could not name the latest one: it is rate limited until {}",
            crate::report::display_time(until)
        ))
    );
    assert_eq!(
        deps_dev_source(&report).status,
        SourceStatus::RateLimited {
            until,
            origin: BlockOrigin::Saved
        }
    );
}

#[tokio::test]
async fn a_deps_dev_block_and_an_osv_answer_both_reach_the_file() {
    let server = MockServer::start();
    server.route(
        "GET",
        SERDE_PACKAGE,
        Reply::new(429, "").with_header("retry-after", "120"),
    );
    server.route("POST", QUERYBATCH, batch(json!([{}])));
    let dir = tempfile::tempdir().unwrap();
    let path = limits_file(&dir);
    let now = Utc::now();
    save_changes(
        Some(&path),
        &[(
            SourceName::Osv,
            Change::Blocked {
                since: now - TimeDelta::minutes(10),
                until: now - TimeDelta::minutes(5),
            },
        )],
        now,
    )
    .await;

    let report = checker_with(server.base(), server.base(), Some(path.clone()))
        .check(request(&["serde", "tokio@1.38.0"]), fixed_clock(now))
        .await;

    let on_disk = load(Some(&path), now).await;
    assert_eq!(on_disk.entry(SourceName::Osv), None);
    assert_eq!(
        on_disk.blocked_until(SourceName::DepsDev, now),
        Some(now + TimeDelta::seconds(120))
    );
    assert_eq!(
        deps_dev_source(&report).status,
        SourceStatus::RateLimited {
            until: now + TimeDelta::seconds(120),
            origin: BlockOrigin::ThisRun
        }
    );
    assert!(
        server
            .received_at("/v3/systems/cargo/packages/tokio/versions/1.38.0")
            .is_empty()
    );
}

#[tokio::test]
async fn a_failed_lookup_for_the_only_package_fails_the_check_naming_deps_dev() {
    let server = MockServer::start();
    server.route("GET", SERDE_PACKAGE, Reply::new(503, ""));

    let report = checker_with(server.base(), server.base(), None)
        .check(request(&["serde"]), fixed_clock(Utc::now()))
        .await;

    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(
        report.packages[0].reason.as_deref(),
        Some("no version was given, and deps.dev could not name the latest one: it answered 503")
    );
    assert_eq!(
        deps_dev_source(&report).status,
        SourceStatus::Failed {
            failure: SourceFailure::Status { code: 503 }
        }
    );
    assert_eq!(
        deps_dev_source(&report).failures,
        [FailureGroup {
            reason: "answered 503".to_owned(),
            packages: 1
        }]
    );
    assert_eq!(osv_source(&report).status, SourceStatus::NotAsked);
}

fn run_with(answers: Vec<PackageAnswer>, block: Option<DateTime<Utc>>) -> SourceRun {
    SourceRun {
        block,
        answers: answers
            .into_iter()
            .enumerate()
            .map(|(index, answer)| (PackageKey(index), answer))
            .collect(),
        ..SourceRun::empty(SourceName::Osv)
    }
}

fn answered() -> PackageAnswer {
    PackageAnswer::Answered {
        advisories: Vec::new(),
    }
}

fn failed(failure: SourceFailure) -> PackageAnswer {
    PackageAnswer::Failed {
        failure,
        advisories: Vec::new(),
    }
}

#[test]
fn source_status_saved_block_is_rate_limited_from_an_earlier_run() {
    let until = Utc::now();

    let status = source_status(
        &run_with(vec![failed(SourceFailure::TimedOut)], None),
        Some(until),
    );

    assert_eq!(
        status,
        SourceStatus::RateLimited {
            until,
            origin: BlockOrigin::Saved
        }
    );
}

#[test]
fn source_status_no_packages_is_not_asked() {
    assert_eq!(
        source_status(&run_with(Vec::new(), None), None),
        SourceStatus::NotAsked
    );
}

#[test]
fn source_status_blocked_before_any_answer_is_rate_limited_this_run() {
    let until = Utc::now();

    let status = source_status(
        &run_with(
            vec![failed(SourceFailure::RateLimited { until })],
            Some(until),
        ),
        None,
    );

    assert_eq!(
        status,
        SourceStatus::RateLimited {
            until,
            origin: BlockOrigin::ThisRun
        }
    );
}

#[test]
fn source_status_nothing_answered_is_failed_with_the_most_frequent_failure() {
    let run = run_with(
        vec![
            failed(SourceFailure::Status { code: 500 }),
            failed(SourceFailure::TimedOut),
            failed(SourceFailure::TimedOut),
        ],
        None,
    );

    assert_eq!(
        source_status(&run, None),
        SourceStatus::Failed {
            failure: SourceFailure::TimedOut
        }
    );
}

#[test]
fn source_status_a_tie_goes_to_the_first_package_s_failure() {
    let run = run_with(
        vec![
            failed(SourceFailure::Status { code: 500 }),
            failed(SourceFailure::TimedOut),
        ],
        None,
    );

    assert_eq!(
        source_status(&run, None),
        SourceStatus::Failed {
            failure: SourceFailure::Status { code: 500 }
        }
    );
}

#[test]
fn source_status_some_answered_is_partial() {
    let run = run_with(vec![answered(), failed(SourceFailure::TimedOut)], None);

    assert_eq!(
        source_status(&run, None),
        SourceStatus::Partial {
            blocked_until: None
        }
    );
}

#[test]
fn source_status_every_package_answered_with_a_late_block_is_answered_with_it() {
    let until = Utc::now();
    let run = run_with(vec![answered(), PackageAnswer::NotFound], Some(until));

    assert_eq!(
        source_status(&run, None),
        SourceStatus::Answered {
            blocked_until: Some(until)
        }
    );
}

fn assert_send<T: Send>(_: &T) {}

#[test]
fn the_check_future_is_send() {
    let checker = checker(closed_port(), None);

    let future = checker.check(request(&["tokio@1.20.0"]), fixed_clock(Utc::now()));

    assert_send(&future);
}
