use super::*;
use crate::rate_limits::{Block, load};
use crate::test_support::{MockServer, Reply, closed_port, fixed_clock};
use chrono::TimeDelta;
use serde_json::json;

const TOOL: &str = "dep-check 1.0.0";
const QUERYBATCH: &str = "/v1/querybatch";

fn checker(base: Url, limits_path: Option<PathBuf>) -> Checker {
    Checker::with_bases(TOOL, "dep-check-test/1.0", Bases { osv: base }, limits_path).unwrap()
}

fn spec(text: &str) -> PackageSpec {
    crate::package::parse_spec(Ecosystem::Cargo, text).unwrap()
}

fn request(texts: &[&str]) -> CheckRequest {
    CheckRequest {
        ecosystem: Ecosystem::Cargo,
        packages: texts.iter().map(|text| spec(text)).collect(),
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
async fn a_package_without_a_version_is_not_checked_and_not_asked_about() {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, batch(json!([{}])));

    let report = checker(server.base(), None)
        .check(request(&["serde", "tokio@1.38.0"]), fixed_clock(Utc::now()))
        .await;

    let queries = server.received_at(QUERYBATCH)[0].json()["queries"].clone();
    assert_eq!(queries.as_array().unwrap().len(), 1);
    assert_eq!(queries[0]["package"]["name"], "tokio");
    assert_eq!(report.packages[0].status, PackageStatus::NotChecked);
    assert_eq!(
        report.packages[0].reason.as_deref(),
        Some("no version was given; finding the latest version needs deps.dev")
    );
    assert_eq!(
        report.packages[0].answers[&SourceName::Osv],
        AnswerWord::NotAsked
    );
    assert_eq!(report.outcome, Outcome::Incomplete);
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
