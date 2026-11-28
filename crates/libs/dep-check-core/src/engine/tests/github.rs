//! The GitHub Advisory Database inside a whole check: the merge, saved blocks, and strikes.

use super::*;

fn github_source(report: &Report) -> &SourceReport {
    &report.sources[2]
}

fn all_on(server: &MockServer) -> Bases {
    Bases {
        osv: server.base(),
        deps_dev: server.base(),
        github: server.base(),
    }
}

/// Saves a GitHub entry with `strikes` whose block ended ten minutes before `now`, as runs an
/// hour ago leave it.
async fn lingering_github_entry(path: &std::path::Path, now: DateTime<Utc>, strikes: u32) {
    for _ in 0..strikes {
        save_changes(
            Some(path),
            &[(
                SourceName::GitHub,
                Change::Blocked {
                    since: now - TimeDelta::minutes(50),
                    until: now - TimeDelta::minutes(10),
                },
            )],
            now,
        )
        .await;
    }
}

#[tokio::test]
async fn one_advisory_from_all_three_sources_is_one_finding_with_github_s_label() {
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
        Reply::json(
            200,
            &json!({"cvss3Score": 9.8, "cvss3Vector": "CVSS:3.1/AV:N"}),
        ),
    );
    server.route(
        "GET",
        "/advisories",
        Reply::json(
            200,
            &json!([{
                "ghsa_id": "GHSA-1", "type": "reviewed", "severity": "low",
                "vulnerabilities": [{"package": {"ecosystem": "rust", "name": "serde"}}],
                "epss": {"percentage": 0.5, "percentile": 0.9}
            }]),
        ),
    );
    server.route("GET", "/advisories", Reply::json(200, &json!([])));

    let report = checker_of(all_on(&server), None)
        .check(request(&["serde@1.0.215"]), fixed_clock(Utc::now()))
        .await;

    let findings = &report.packages[0].findings;
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].id, "RUSTSEC-1");
    assert_eq!(
        findings[0].sources,
        [SourceName::Osv, SourceName::DepsDev, SourceName::GitHub]
    );
    assert_eq!(findings[0].severity, Severity::Low);
    assert_eq!(
        findings[0].epss.as_ref().map(|epss| epss.percentile),
        Some(0.9)
    );
}

#[tokio::test]
async fn a_github_advisory_makes_a_package_vulnerable_while_osv_is_down() {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, Reply::new(503, ""));
    server.route("GET", SERDE_VERSION, Reply::json(200, &json!({})));
    server.route(
        "GET",
        "/advisories",
        Reply::json(
            200,
            &json!([{
                "ghsa_id": "GHSA-1", "type": "reviewed", "severity": "high",
                "vulnerabilities": [{"package": {"ecosystem": "rust", "name": "serde"}}]
            }]),
        ),
    );
    server.route("GET", "/advisories", Reply::json(200, &json!([])));

    let report = checker_of(all_on(&server), None)
        .check(
            request(&["serde@1.0.215", "tokio@1.20.0"]),
            fixed_clock(Utc::now()),
        )
        .await;

    assert_eq!(report.packages[0].status, PackageStatus::Vulnerable);
    assert_eq!(report.packages[0].findings[0].id, "GHSA-1");
    assert_eq!(report.packages[1].status, PackageStatus::Clean);
    assert_eq!(report.outcome, Outcome::Vulnerable);
}

#[tokio::test]
async fn a_saved_github_block_leaves_the_outcome_to_the_others() {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, batch(json!([{}])));
    server.route("GET", SERDE_VERSION, Reply::json(200, &json!({})));
    let dir = tempfile::tempdir().unwrap();
    let path = limits_file(&dir);
    let now = Utc::now();
    let until = now + TimeDelta::minutes(30);
    save_changes(
        Some(&path),
        &[(SourceName::GitHub, Change::Blocked { since: now, until })],
        now,
    )
    .await;

    let report = checker_of(all_on(&server), Some(path))
        .check(request(&["serde@1.0.215"]), fixed_clock(now))
        .await;

    assert!(server.received_at("/advisories").is_empty());
    assert_eq!(report.outcome, Outcome::Clean);
    assert_eq!(
        github_source(&report).status,
        SourceStatus::RateLimited {
            until,
            origin: BlockOrigin::Saved
        }
    );
    assert_eq!(
        report.packages[0].answers[&SourceName::GitHub],
        AnswerWord::RateLimited
    );
}

#[tokio::test]
async fn a_github_answer_that_uses_the_last_request_saves_the_block() {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, batch(json!([{}])));
    server.route("GET", SERDE_VERSION, Reply::json(200, &json!({})));
    let now = DateTime::from_timestamp(Utc::now().timestamp(), 0).unwrap();
    let reset = now + TimeDelta::minutes(20);
    server.route("GET", "/advisories", Reply::json(200, &json!([])));
    server.route(
        "GET",
        "/advisories",
        Reply::json(200, &json!([]))
            .with_header("x-ratelimit-remaining", "0")
            .with_header("x-ratelimit-reset", &reset.timestamp().to_string()),
    );
    let dir = tempfile::tempdir().unwrap();
    let path = limits_file(&dir);
    lingering_github_entry(&path, now, 3).await;

    let report = checker_of(all_on(&server), Some(path.clone()))
        .check(request(&["serde@1.0.215"]), fixed_clock(now))
        .await;

    assert_eq!(
        github_source(&report).status,
        SourceStatus::Answered {
            blocked_until: Some(reset)
        }
    );
    let saved = load(Some(&path), now).await;
    assert_eq!(saved.blocked_until(SourceName::GitHub, now), Some(reset));
    assert_eq!(saved.strikes(SourceName::GitHub), 1);
}

#[tokio::test]
async fn the_strikes_of_a_block_that_ended_double_github_s_next_wait() {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, batch(json!([{}])));
    server.route("GET", SERDE_VERSION, Reply::json(200, &json!({})));
    server.route("GET", "/advisories", Reply::new(403, ""));
    let now = DateTime::from_timestamp(Utc::now().timestamp(), 0).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = limits_file(&dir);
    lingering_github_entry(&path, now, 3).await;

    let report = checker_of(all_on(&server), Some(path.clone()))
        .check(request(&["serde@1.0.215"]), fixed_clock(now))
        .await;

    let until = now + TimeDelta::minutes(8);
    assert_eq!(
        github_source(&report).status,
        SourceStatus::RateLimited {
            until,
            origin: BlockOrigin::ThisRun
        }
    );
    let saved = load(Some(&path), now).await;
    assert_eq!(saved.blocked_until(SourceName::GitHub, now), Some(until));
    assert_eq!(saved.strikes(SourceName::GitHub), 4);
}
