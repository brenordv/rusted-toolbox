use super::*;
use crate::http::build_client;
use crate::test_support::{MockServer, Reply, fixed_clock};
use chrono::TimeDelta;
use rstest::rstest;
use serde_json::json;

const ADVISORIES: &str = "/advisories";
const AGENT: &str = "dep-check-test/1.0";

mod confirms;
mod paging;

fn client_for(base: Url) -> GitHubClient {
    let limits = HttpLimits::default();
    let client = build_client(&limits, AGENT, github_headers()).unwrap();
    GitHubClient::new(client, base, limits).unwrap()
}

fn resolved(index: usize, name: &str, version: &str) -> Resolved {
    Resolved {
        key: PackageKey(index),
        name: name.to_owned(),
        version: version.to_owned(),
    }
}

fn context_at(now: DateTime<Utc>, strikes: u32) -> RunContext {
    RunContext {
        clock: fixed_clock(now),
        strikes,
    }
}

fn advisory(ghsa: &str, kind: &str, names: &[&str]) -> Value {
    let vulnerabilities: Vec<Value> = names
        .iter()
        .map(|name| {
            json!({
                "package": {"ecosystem": "rust", "name": name},
                "vulnerable_version_range": "< 2.0.0",
                "first_patched_version": format!("2.0.0-{name}")
            })
        })
        .collect();
    json!({
        "ghsa_id": ghsa,
        "cve_id": null,
        "html_url": format!("https://github.com/advisories/{ghsa}"),
        "summary": format!("summary of {ghsa}"),
        "type": kind,
        "severity": "high",
        "published_at": "2025-04-07T21:12:50Z",
        "withdrawn_at": null,
        "identifiers": [{"value": ghsa, "type": "GHSA"}],
        "vulnerabilities": vulnerabilities,
        "cvss_severities": null,
        "epss": null
    })
}

fn page(advisories: Vec<Value>) -> Reply {
    Reply::json(200, &Value::Array(advisories))
}

fn empty_page() -> Reply {
    page(Vec::new())
}

async fn check(server: &MockServer, ecosystem: Ecosystem, packages: &[Resolved]) -> SourceRun {
    client_for(server.base())
        .check(ecosystem, packages, &context_at(Utc::now(), 0))
        .await
}

fn queries(server: &MockServer) -> Vec<String> {
    server
        .received_at(ADVISORIES)
        .into_iter()
        .map(|request| request.query)
        .collect()
}

#[tokio::test]
async fn every_request_carries_the_agent_the_accept_type_and_the_api_version() {
    let server = MockServer::start();
    server.route("GET", ADVISORIES, empty_page());

    check(&server, Ecosystem::Cargo, &[resolved(0, "tokio", "1.20.0")]).await;

    let request = &server.received()[0];
    assert_eq!(request.header("user-agent"), Some(AGENT));
    assert_eq!(
        request.header("accept"),
        Some("application/vnd.github+json")
    );
    assert_eq!(request.header("x-github-api-version"), Some("2026-03-10"));
}

#[rstest]
#[case::cargo(Ecosystem::Cargo, "rust")]
#[case::nuget(Ecosystem::Nuget, "nuget")]
#[case::pypi(Ecosystem::Pypi, "pip")]
#[case::npm(Ecosystem::Npm, "npm")]
#[case::rubygems(Ecosystem::Rubygems, "rubygems")]
#[tokio::test]
async fn one_chunk_asks_reviewed_then_malware(#[case] ecosystem: Ecosystem, #[case] word: &str) {
    let server = MockServer::start();
    server.route("GET", ADVISORIES, empty_page());

    check(&server, ecosystem, &[resolved(0, "pkg", "1.0.0")]).await;

    let sent = queries(&server);
    assert_eq!(sent.len(), 2);
    assert!(
        sent[0].starts_with(&format!("ecosystem={word}&type=reviewed&per_page=100&")),
        "{}",
        sent[0]
    );
    assert!(
        sent[1].starts_with(&format!("ecosystem={word}&type=malware&per_page=100&")),
        "{}",
        sent[1]
    );
}

#[tokio::test]
async fn affects_joins_the_encoded_specs_with_literal_commas() {
    let server = MockServer::start();
    server.route("GET", ADVISORIES, empty_page());

    check(
        &server,
        Ecosystem::Npm,
        &[
            resolved(0, "@types/node", "20.1.0"),
            resolved(1, "left-pad", "1.3.0"),
        ],
    )
    .await;

    let query = &queries(&server)[0];
    assert!(
        query.ends_with("&affects=%40types%2Fnode%4020.1.0,left-pad%401.3.0"),
        "{query}"
    );
    assert!(
        query
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._~%,=&".contains(c)),
        "{query}"
    );
}

#[tokio::test]
async fn packages_past_100_go_into_a_second_chunk() {
    let server = MockServer::start();
    server.route("GET", ADVISORIES, empty_page());
    let packages: Vec<Resolved> = (0..101)
        .map(|index| resolved(index, &format!("crate{index}"), "1.0.0"))
        .collect();

    check(&server, Ecosystem::Cargo, &packages).await;

    assert_eq!(queries(&server).len(), 4);
}

#[tokio::test]
async fn two_versions_of_one_name_go_into_two_chunks() {
    let server = MockServer::start();
    server.route("GET", ADVISORIES, empty_page());

    check(
        &server,
        Ecosystem::Cargo,
        &[
            resolved(0, "tokio", "1.20.0"),
            resolved(1, "tokio", "1.38.0"),
        ],
    )
    .await;

    let sent = queries(&server);
    assert_eq!(sent.len(), 4);
    assert!(sent[0].ends_with("affects=tokio%401.20.0"), "{}", sent[0]);
    assert!(sent[2].ends_with("affects=tokio%401.38.0"), "{}", sent[2]);
}

#[tokio::test]
async fn a_403_with_retry_after_blocks_and_sends_nothing_more() {
    let server = MockServer::start();
    server.route(
        "GET",
        ADVISORIES,
        Reply::new(403, "").with_header("retry-after", "30"),
    );
    let now = Utc::now();
    let packages: Vec<Resolved> = (0..101)
        .map(|index| resolved(index, &format!("crate{index}"), "1.0.0"))
        .collect();

    let run = client_for(server.base())
        .check(Ecosystem::Cargo, &packages, &context_at(now, 0))
        .await;

    let until = now + TimeDelta::seconds(30);
    assert_eq!(queries(&server).len(), 1);
    assert_eq!(run.block, Some(until));
    assert_eq!(
        run.answers[&PackageKey(100)].failure(),
        Some(&SourceFailure::RateLimited { until })
    );
}

#[tokio::test]
async fn a_403_with_no_requests_left_blocks_until_the_reset() {
    let server = MockServer::start();
    let now = Utc::now();
    let reset = now + TimeDelta::minutes(10);
    server.route(
        "GET",
        ADVISORIES,
        Reply::new(403, "")
            .with_header("x-ratelimit-remaining", "0")
            .with_header("x-ratelimit-reset", &reset.timestamp().to_string()),
    );

    let run = client_for(server.base())
        .check(
            Ecosystem::Cargo,
            &[resolved(0, "tokio", "1.20.0")],
            &context_at(now, 0),
        )
        .await;

    assert_eq!(run.block, DateTime::from_timestamp(reset.timestamp(), 0));
}

#[rstest]
#[case::no_strikes(0, TimeDelta::minutes(1))]
#[case::three_strikes(3, TimeDelta::minutes(8))]
#[case::many_strikes(40, TimeDelta::hours(1))]
#[tokio::test]
async fn a_bare_403_waits_a_minute_doubled_per_saved_strike(
    #[case] strikes: u32,
    #[case] wait: TimeDelta,
) {
    let server = MockServer::start();
    server.route("GET", ADVISORIES, Reply::new(403, ""));
    let now = Utc::now();

    let run = client_for(server.base())
        .check(
            Ecosystem::Cargo,
            &[resolved(0, "tokio", "1.20.0")],
            &context_at(now, strikes),
        )
        .await;

    assert_eq!(run.block, Some(now + wait));
}

#[tokio::test]
async fn a_200_with_no_requests_left_is_used_and_then_blocks() {
    let server = MockServer::start();
    let now = Utc::now();
    let reset = now + TimeDelta::minutes(20);
    server.route("GET", ADVISORIES, empty_page());
    server.route(
        "GET",
        ADVISORIES,
        page(vec![advisory("GHSA-mal", "malware", &["tokio"])])
            .with_header("x-ratelimit-remaining", "0")
            .with_header("x-ratelimit-reset", &reset.timestamp().to_string()),
    );

    let run = client_for(server.base())
        .check(
            Ecosystem::Cargo,
            &[resolved(0, "tokio", "1.20.0")],
            &context_at(now, 0),
        )
        .await;

    assert_eq!(run.block, DateTime::from_timestamp(reset.timestamp(), 0));
    let answer = &run.answers[&PackageKey(0)];
    assert!(
        matches!(answer, PackageAnswer::Answered { .. }),
        "{answer:?}"
    );
    assert_eq!(answer.advisories()[0].kind, AdvisoryKind::Malware);
}

#[tokio::test]
async fn a_200_that_leaves_no_requests_stops_before_the_next_one() {
    let server = MockServer::start();
    let now = Utc::now();
    let reset = now + TimeDelta::minutes(20);
    server.route(
        "GET",
        ADVISORIES,
        page(vec![advisory("GHSA-a", "reviewed", &["tokio"])])
            .with_header("x-ratelimit-remaining", "0")
            .with_header("x-ratelimit-reset", &reset.timestamp().to_string()),
    );

    let run = client_for(server.base())
        .check(
            Ecosystem::Cargo,
            &[resolved(0, "tokio", "1.20.0")],
            &context_at(now, 0),
        )
        .await;

    let until = DateTime::from_timestamp(reset.timestamp(), 0).unwrap();
    assert_eq!(queries(&server).len(), 1);
    let answer = &run.answers[&PackageKey(0)];
    assert_eq!(
        answer.failure(),
        Some(&SourceFailure::RateLimited { until })
    );
    assert_eq!(answer.advisories()[0].id, "GHSA-a");
}

#[tokio::test]
async fn a_200_with_requests_left_does_not_block() {
    let server = MockServer::start();
    server.route(
        "GET",
        ADVISORIES,
        empty_page().with_header("x-ratelimit-remaining", "5"),
    );

    let run = check(&server, Ecosystem::Cargo, &[resolved(0, "tokio", "1.20.0")]).await;

    assert_eq!(run.block, None);
}

#[test]
fn an_advisory_maps_its_fields_and_skips_unknown_labels() {
    let mut value = advisory("GHSA-x", "malware", &["tokio", "other"]);
    value["severity"] = json!("unknown");
    value["cve_id"] = json!("CVE-2026-1");
    value["identifiers"] =
        json!([{"value": "GHSA-x", "type": "GHSA"}, {"value": "CVE-2026-1", "type": "CVE"}]);
    value["cvss_severities"] = json!({
        "cvss_v3": {"vector_string": null, "score": 0.0},
        "cvss_v4": {"vector_string": "CVSS:4.0/AV:N", "score": 2.7}
    });
    value["epss"] = json!({"percentage": 0.00045, "percentile": "0.16"});
    let parsed: GhAdvisory = serde_json::from_value(value).unwrap();

    let mapped = advisory_for(&parsed, Ecosystem::Cargo, "tokio");

    assert_eq!(mapped.kind, AdvisoryKind::Malware);
    assert_eq!(mapped.severity, None);
    assert_eq!(mapped.aliases, ["CVE-2026-1"]);
    assert_eq!(mapped.fixed_in, ["2.0.0-tokio"]);
    assert_eq!(
        mapped.cvss,
        [Cvss {
            version: "4".to_owned(),
            vector: Some("CVSS:4.0/AV:N".to_owned()),
            score: Some(2.7),
            from: SourceName::GitHub
        }]
    );
    assert_eq!(
        mapped.epss,
        Some(Epss {
            percentage: 0.00045,
            percentile: 0.16
        })
    );
}

#[test]
fn null_lists_parse() {
    let value = json!({
        "ghsa_id": "GHSA-x", "cve_id": null, "html_url": null, "summary": null, "type": "reviewed",
        "severity": null, "published_at": null, "withdrawn_at": null, "identifiers": null,
        "vulnerabilities": null, "cvss_severities": null, "epss": null
    });

    let parsed: GhAdvisory = serde_json::from_value(value).unwrap();

    assert_eq!(
        advisory_for(&parsed, Ecosystem::Cargo, "x").aliases,
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn a_withdrawn_advisory_is_skipped() {
    let server = MockServer::start();
    let mut withdrawn = advisory("GHSA-gone", "reviewed", &["tokio"]);
    withdrawn["withdrawn_at"] = json!("2025-05-01T00:00:00Z");
    server.route("GET", ADVISORIES, page(vec![withdrawn]));
    server.route("GET", ADVISORIES, empty_page());

    let run = check(&server, Ecosystem::Cargo, &[resolved(0, "tokio", "1.20.0")]).await;

    assert_eq!(
        run.answers[&PackageKey(0)],
        PackageAnswer::Answered {
            advisories: Vec::new()
        }
    );
}

#[test]
fn the_first_patched_version_comes_from_this_package_s_entries_only() {
    let mut value = advisory("GHSA-x", "reviewed", &["tokio"]);
    value["vulnerabilities"] = json!([
        {"package": {"ecosystem": "rust", "name": "tokio"}, "first_patched_version": "1.38.2"},
        {"package": {"ecosystem": "rust", "name": "tokio"}, "first_patched_version": "1.44.2"},
        {"package": {"ecosystem": "rust", "name": "tokio-util"}, "first_patched_version": "0.7.9"},
        {"package": {"ecosystem": "npm", "name": "tokio"}, "first_patched_version": "9.9.9"}
    ]);
    let parsed: GhAdvisory = serde_json::from_value(value).unwrap();

    let mapped = advisory_for(&parsed, Ecosystem::Cargo, "tokio");

    assert_eq!(mapped.fixed_in, ["1.38.2", "1.44.2"]);
}

#[tokio::test]
async fn an_advisory_naming_no_chunk_package_fails_the_chunk() {
    let server = MockServer::start();
    server.route(
        "GET",
        ADVISORIES,
        page(vec![advisory("GHSA-x", "reviewed", &["something-else"])]),
    );

    let run = check(&server, Ecosystem::Cargo, &[resolved(0, "tokio", "1.20.0")]).await;

    assert_eq!(
        run.answers[&PackageKey(0)].failure(),
        Some(&SourceFailure::BadAnswer {
            detail: "an advisory named none of the packages asked".to_owned()
        })
    );
}

#[rstest]
#[case::unprocessable(Reply::new(422, ""), SourceFailure::Status { code: 422 })]
#[case::unavailable(Reply::new(503, ""), SourceFailure::Status { code: 503 })]
#[tokio::test]
async fn an_error_status_fails_the_chunk_without_a_block(
    #[case] reply: Reply,
    #[case] failure: SourceFailure,
) {
    let server = MockServer::start();
    server.route("GET", ADVISORIES, reply);

    let run = check(&server, Ecosystem::Cargo, &[resolved(0, "tokio", "1.20.0")]).await;

    assert_eq!(run.answers[&PackageKey(0)].failure(), Some(&failure));
    assert_eq!(run.block, None);
}

#[tokio::test]
async fn a_malformed_page_fails_the_chunk_without_a_block() {
    let server = MockServer::start();
    server.route("GET", ADVISORIES, Reply::new(200, "{ not a list"));

    let run = check(&server, Ecosystem::Cargo, &[resolved(0, "tokio", "1.20.0")]).await;

    assert!(matches!(
        run.answers[&PackageKey(0)].failure(),
        Some(SourceFailure::BadAnswer { .. })
    ));
    assert_eq!(run.block, None);
}

/// Advisories GitHub lists for these versions, and a version each one does not affect: a plain
/// crate, a scoped npm name, and mixed-case PyPI and NuGet names spelled as GitHub spells them.
/// Picked from the advisory database on 2026-11-26.
const LIVE_SHAPES: [(Ecosystem, &str, &str, &str, &str); 4] = [
    (
        Ecosystem::Cargo,
        "tokio",
        "1.20.0",
        "1.23.1",
        "GHSA-7rrj-xr53-82p7",
    ),
    (
        Ecosystem::Npm,
        "@babel/traverse",
        "7.22.0",
        "7.23.2",
        "GHSA-67hx-6x53-jw92",
    ),
    (
        Ecosystem::Pypi,
        "Django",
        "3.2.0",
        "4.2.24",
        "GHSA-6w2r-r2m5-xq5w",
    ),
    (
        Ecosystem::Nuget,
        "Newtonsoft.Json",
        "12.0.1",
        "13.0.1",
        "GHSA-5crp-9r3c-p9vr",
    ),
];

#[tokio::test]
#[ignore = "asks the real GitHub API (16 of the hour's 60 requests); run with --ignored live_"]
async fn live_github_affects_by_shape() {
    let client = client_for(Url::parse(GITHUB_BASE).unwrap());
    let context = context_at(Utc::now(), 0);

    for (ecosystem, name, vulnerable, fixed, ghsa) in LIVE_SHAPES {
        let run = client
            .check(
                ecosystem,
                &[resolved(0, name, vulnerable), resolved(1, name, fixed)],
                &context,
            )
            .await;
        let ids = |key: usize| -> Vec<String> {
            run.answers[&PackageKey(key)]
                .advisories()
                .iter()
                .map(|advisory| advisory.id.clone())
                .collect()
        };
        assert!(
            ids(0).contains(&ghsa.to_owned()),
            "{name}@{vulnerable}: {:?}",
            ids(0)
        );
        assert!(
            !ids(1).contains(&ghsa.to_owned()),
            "{name}@{fixed}: {:?}",
            ids(1)
        );
    }
}

#[tokio::test]
#[ignore = "asks the real GitHub API (2 requests); run with --ignored live_"]
async fn live_github_matches_names_as_spelled() {
    let client = client_for(Url::parse(GITHUB_BASE).unwrap());

    let run = client
        .check(
            Ecosystem::Nuget,
            &[resolved(0, "newtonsoft.json", "12.0.1")],
            &context_at(Utc::now(), 0),
        )
        .await;

    // When this fails GitHub has started to fold case, and the readmes' sentence on spelling
    // is out of date.
    assert_eq!(
        run.answers[&PackageKey(0)],
        PackageAnswer::Answered {
            advisories: Vec::new()
        }
    );
}

#[tokio::test]
#[ignore = "asks the real GitHub API (2 requests); run with --ignored live_"]
async fn live_github_hundred_packages() {
    let client = client_for(Url::parse(GITHUB_BASE).unwrap());
    let packages: Vec<Resolved> = (0..100)
        .map(|index| resolved(index, &format!("zz-dep-check-48151-{index}"), "1.0.0"))
        .collect();

    let run = client
        .check(Ecosystem::Cargo, &packages, &context_at(Utc::now(), 0))
        .await;

    assert!(
        run.answers
            .values()
            .all(|answer| matches!(answer, PackageAnswer::Answered { .. }))
    );
    assert_eq!(run.requests, 2);
}
