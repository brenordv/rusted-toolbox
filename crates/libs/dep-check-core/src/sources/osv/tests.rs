use super::*;
use crate::advisory::AdvisoryKind;
use crate::engine::PackageKey;
use crate::http::build_client;
use crate::test_support::{MockServer, Reply, fixed_clock};
use reqwest::header::HeaderMap;
use serde_json::{Value, json};

const QUERYBATCH: &str = "/v1/querybatch";

fn client_for(base: Url) -> OsvClient {
    let limits = HttpLimits::default();
    let client = build_client(&limits, "dep-check-test/1.0", HeaderMap::new()).unwrap();
    OsvClient::new(client, base, limits).unwrap()
}

fn resolved(index: usize, name: &str, version: &str) -> Resolved {
    Resolved {
        key: PackageKey(index),
        name: name.to_owned(),
        version: version.to_owned(),
    }
}

fn many(count: usize) -> Vec<Resolved> {
    (0..count)
        .map(|index| resolved(index, &format!("crate{index}"), "1.0.0"))
        .collect()
}

fn context() -> RunContext {
    RunContext {
        clock: fixed_clock(Utc::now()),
        strikes: 0,
    }
}

fn record(id: &str) -> Value {
    json!({"id": id, "summary": format!("summary of {id}"), "modified": "2026-01-01T00:00:00Z"})
}

fn batch(results: Vec<Value>) -> Reply {
    Reply::json(200, &json!({ "results": results }))
}

fn with_vulns(ids: &[&str]) -> Value {
    json!({ "vulns": ids.iter().map(|id| json!({"id": id, "modified": "2026-01-01T00:00:00Z"})).collect::<Vec<_>>() })
}

fn empty_results(count: usize) -> Vec<Value> {
    vec![json!({}); count]
}

fn ids_of(answer: &PackageAnswer) -> Vec<&str> {
    answer
        .advisories()
        .iter()
        .map(|advisory| advisory.id.as_str())
        .collect()
}

async fn check(server: &MockServer, ecosystem: Ecosystem, packages: &[Resolved]) -> SourceRun {
    client_for(server.base())
        .check(ecosystem, packages, &context())
        .await
}

#[tokio::test]
async fn querybatch_body_has_the_documented_shape_and_paths_are_joined() {
    let server = MockServer::start();
    server.route(
        "POST",
        QUERYBATCH,
        batch(vec![with_vulns(&["RUSTSEC-2023-0005"]), json!({})]),
    );
    server.route(
        "GET",
        "/v1/vulns/RUSTSEC-2023-0005",
        Reply::json(200, &record("RUSTSEC-2023-0005")),
    );
    let packages = [
        resolved(0, "tokio", "1.20.0"),
        resolved(1, "serde", "1.0.215"),
    ];

    let run = check(&server, Ecosystem::Cargo, &packages).await;

    assert_eq!(
        server.received_at(QUERYBATCH)[0].json(),
        json!({"queries": [
            {"package": {"name": "tokio", "ecosystem": "crates.io"}, "version": "1.20.0"},
            {"package": {"name": "serde", "ecosystem": "crates.io"}, "version": "1.0.215"}
        ]})
    );
    let paths: Vec<String> = server.received().into_iter().map(|r| r.path).collect();
    assert_eq!(paths, [QUERYBATCH, "/v1/vulns/RUSTSEC-2023-0005"]);
    assert_eq!(ids_of(&run.answers[&PackageKey(0)]), ["RUSTSEC-2023-0005"]);
    assert_eq!(
        run.answers[&PackageKey(0)].advisories()[0]
            .summary
            .as_deref(),
        Some("summary of RUSTSEC-2023-0005")
    );
    assert_eq!(
        run.answers[&PackageKey(1)],
        PackageAnswer::Answered {
            advisories: Vec::new()
        }
    );
    assert_eq!((run.requests, run.block, run.details_missing), (2, None, 0));
}

#[tokio::test]
async fn an_id_two_packages_share_is_fetched_once() {
    let server = MockServer::start();
    server.route(
        "POST",
        QUERYBATCH,
        batch(vec![
            with_vulns(&["GHSA-shared"]),
            with_vulns(&["GHSA-shared"]),
        ]),
    );
    server.route(
        "GET",
        "/v1/vulns/GHSA-shared",
        Reply::json(200, &record("GHSA-shared")),
    );

    let run = check(&server, Ecosystem::Npm, &many(2)).await;

    assert_eq!(server.received_at("/v1/vulns/GHSA-shared").len(), 1);
    assert_eq!(ids_of(&run.answers[&PackageKey(1)]), ["GHSA-shared"]);
}

#[tokio::test]
async fn a_mal_id_is_malware() {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, batch(vec![with_vulns(&["MAL-2026-1"])]));
    server.route(
        "GET",
        "/v1/vulns/MAL-2026-1",
        Reply::json(200, &record("MAL-2026-1")),
    );

    let run = check(&server, Ecosystem::Npm, &many(1)).await;

    assert_eq!(
        run.answers[&PackageKey(0)].advisories()[0].kind,
        AdvisoryKind::Malware
    );
}

fn parse_record(value: Value) -> OsvRecord {
    serde_json::from_value(value).unwrap()
}

#[test]
fn fixed_in_takes_only_this_package_s_events_and_matches_pypi_spellings() {
    let record = parse_record(json!({
        "id": "PYSEC-1",
        "affected": [
            {"package": {"ecosystem": "PyPI", "name": "Foo_Bar"},
             "ranges": [{"events": [{"introduced": "0"}, {"fixed": "1.2"}, {"fixed": "1.2"}]}]},
            {"package": {"ecosystem": "PyPI", "name": "other"},
             "ranges": [{"events": [{"fixed": "9.9"}]}]},
            {"package": {"ecosystem": "npm", "name": "foo-bar"},
             "ranges": [{"events": [{"fixed": "7.0"}]}]}
        ]
    }));

    let advisory = advisory_from_record(SourceName::Osv, &record, Ecosystem::Pypi, "foo-bar");

    assert_eq!(advisory.fixed_in, ["1.2"]);
}

#[test]
fn only_cvss_severities_become_ratings_with_vectors_and_no_scores() {
    let record = parse_record(json!({
        "id": "X-1",
        "severity": [
            {"type": "CVSS_V3", "score": "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H"},
            {"type": "Ubuntu", "score": "high"},
            {"type": "CVSS_V4", "score": "CVSS:4.0/AV:N"}
        ]
    }));

    let advisory = advisory_from_record(SourceName::Osv, &record, Ecosystem::Cargo, "x");

    assert_eq!(
        advisory.cvss,
        [
            Cvss {
                version: "3".to_owned(),
                vector: Some("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H".to_owned()),
                score: None,
                from: SourceName::Osv
            },
            Cvss {
                version: "4".to_owned(),
                vector: Some("CVSS:4.0/AV:N".to_owned()),
                score: None,
                from: SourceName::Osv
            }
        ]
    );
}

#[test]
fn urls_take_advisories_before_web_pages_and_stop_at_five() {
    let record = parse_record(json!({
        "id": "X-1",
        "references": [
            {"type": "WEB", "url": "https://web.example/1"},
            {"type": "PACKAGE", "url": "https://package.example"},
            {"type": "ADVISORY", "url": "https://advisory.example/1"},
            {"type": "WEB", "url": "https://web.example/2"},
            {"type": "ADVISORY", "url": "https://advisory.example/2"},
            {"type": "WEB", "url": "https://web.example/3"},
            {"type": "WEB", "url": "https://web.example/4"}
        ]
    }));

    let advisory = advisory_from_record(SourceName::Osv, &record, Ecosystem::Cargo, "x");

    assert_eq!(
        advisory.urls,
        [
            "https://advisory.example/1",
            "https://advisory.example/2",
            "https://web.example/1",
            "https://web.example/2",
            "https://web.example/3"
        ]
    );
}

#[tokio::test]
async fn packages_past_1000_go_in_a_second_batch() {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, batch(empty_results(1000)));
    server.route("POST", QUERYBATCH, batch(empty_results(1)));

    let run = check(&server, Ecosystem::Cargo, &many(1001)).await;

    let sizes: Vec<usize> = server
        .received_at(QUERYBATCH)
        .iter()
        .map(|request| request.json()["queries"].as_array().unwrap().len())
        .collect();
    assert_eq!(sizes, [1000, 1]);
    assert!(matches!(
        run.answers[&PackageKey(1000)],
        PackageAnswer::Answered { .. }
    ));
}

#[tokio::test]
async fn a_query_with_more_pages_is_asked_again_alone_with_its_token() {
    let server = MockServer::start();
    server.route(
        "POST",
        QUERYBATCH,
        batch(vec![
            json!({"vulns": [{"id": "X-1"}], "next_page_token": "page-2"}),
            json!({}),
        ]),
    );
    server.route("POST", QUERYBATCH, batch(vec![with_vulns(&["X-2"])]));
    server.route("GET", "/v1/vulns/X-1", Reply::json(200, &record("X-1")));
    server.route("GET", "/v1/vulns/X-2", Reply::json(200, &record("X-2")));

    let run = check(&server, Ecosystem::Cargo, &many(2)).await;

    let second = server.received_at(QUERYBATCH)[1].json();
    assert_eq!(
        second,
        json!({"queries": [
            {"package": {"name": "crate0", "ecosystem": "crates.io"}, "version": "1.0.0", "page_token": "page-2"}
        ]})
    );
    assert_eq!(ids_of(&run.answers[&PackageKey(0)]), ["X-1", "X-2"]);
}

#[tokio::test]
async fn a_query_paging_past_20_pages_fails_its_package_and_keeps_its_ids() {
    let server = MockServer::start();
    server.route(
        "POST",
        QUERYBATCH,
        batch(vec![
            json!({"vulns": [{"id": "X-1"}], "next_page_token": "more"}),
            json!({}),
        ]),
    );
    server.route(
        "POST",
        QUERYBATCH,
        batch(vec![
            json!({"vulns": [{"id": "X-2"}], "next_page_token": "more"}),
        ]),
    );
    server.route("GET", "/v1/vulns/X-1", Reply::json(200, &record("X-1")));
    server.route("GET", "/v1/vulns/X-2", Reply::json(200, &record("X-2")));

    let run = check(&server, Ecosystem::Cargo, &many(2)).await;

    assert_eq!(server.received_at(QUERYBATCH).len(), 20);
    assert_eq!(
        run.answers[&PackageKey(0)].failure(),
        Some(&SourceFailure::BadAnswer {
            detail: "more than 20 pages".to_owned()
        })
    );
    assert_eq!(ids_of(&run.answers[&PackageKey(0)]), ["X-1", "X-2"]);
    assert!(matches!(
        run.answers[&PackageKey(1)],
        PackageAnswer::Answered { .. }
    ));
}

#[tokio::test]
async fn results_of_the_wrong_length_fail_the_chunk() {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, batch(empty_results(1)));

    let run = check(&server, Ecosystem::Cargo, &many(2)).await;

    let mismatch = SourceFailure::BadAnswer {
        detail: "the answer did not match the question".to_owned(),
    };
    assert_eq!(run.answers[&PackageKey(0)].failure(), Some(&mismatch));
    assert_eq!(run.answers[&PackageKey(1)].failure(), Some(&mismatch));
}

#[tokio::test]
async fn a_failed_record_fetch_leaves_an_id_only_advisory_and_the_package_answered() {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, batch(vec![with_vulns(&["X-1", "X-2"])]));
    server.route("GET", "/v1/vulns/X-1", Reply::json(200, &record("X-1")));
    server.route("GET", "/v1/vulns/X-2", Reply::new(503, ""));

    let run = check(&server, Ecosystem::Cargo, &many(1)).await;

    let answer = &run.answers[&PackageKey(0)];
    assert!(matches!(answer, PackageAnswer::Answered { .. }));
    assert_eq!(ids_of(answer), ["X-1", "X-2"]);
    assert_eq!(answer.advisories()[1].summary, None);
    assert_eq!(run.details_missing, 1);
}

#[tokio::test]
async fn a_querybatch_503_fails_every_package_of_its_chunk() {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, Reply::new(503, ""));

    let run = check(&server, Ecosystem::Cargo, &many(2)).await;

    assert_eq!(
        run.answers
            .values()
            .map(PackageAnswer::failure)
            .collect::<Vec<_>>(),
        [
            Some(&SourceFailure::Status { code: 503 }),
            Some(&SourceFailure::Status { code: 503 })
        ]
    );
}

#[tokio::test]
async fn a_block_on_the_second_batch_stops_every_later_request() {
    let server = MockServer::start();
    let mut first = empty_results(1000);
    first[0] = with_vulns(&["X-1"]);
    server.route("POST", QUERYBATCH, batch(first));
    server.route(
        "POST",
        QUERYBATCH,
        Reply::new(429, "").with_header("retry-after", "30"),
    );
    server.route("GET", "/v1/vulns/X-1", Reply::json(200, &record("X-1")));
    let now = Utc::now();
    let blocked_context = RunContext {
        clock: fixed_clock(now),
        strikes: 0,
    };

    let run = client_for(server.base())
        .check(Ecosystem::Cargo, &many(1001), &blocked_context)
        .await;

    let until = now + chrono::TimeDelta::seconds(30);
    assert_eq!(server.received().len(), 2);
    assert_eq!(run.block, Some(until));
    assert!(matches!(
        run.answers[&PackageKey(0)],
        PackageAnswer::Answered { .. }
    ));
    assert_eq!(ids_of(&run.answers[&PackageKey(0)]), ["X-1"]);
    assert_eq!(
        run.answers[&PackageKey(1000)].failure(),
        Some(&SourceFailure::RateLimited { until })
    );
    assert_eq!(run.details_missing, 1);
}

#[tokio::test]
async fn a_block_on_the_first_batch_sends_no_later_batch() {
    let server = MockServer::start();
    server.route(
        "POST",
        QUERYBATCH,
        Reply::new(429, "").with_header("retry-after", "30"),
    );
    let now = Utc::now();
    let blocked_context = RunContext {
        clock: fixed_clock(now),
        strikes: 0,
    };

    let run = client_for(server.base())
        .check(Ecosystem::Cargo, &many(1001), &blocked_context)
        .await;

    let until = now + chrono::TimeDelta::seconds(30);
    assert_eq!(server.received().len(), 1);
    assert_eq!(
        run.answers[&PackageKey(1000)].failure(),
        Some(&SourceFailure::RateLimited { until })
    );
    assert_eq!(
        run.answers[&PackageKey(0)].failure(),
        Some(&SourceFailure::RateLimited { until })
    );
}

#[tokio::test]
async fn a_querybatch_404_fails_every_package_of_its_chunk() {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, Reply::new(404, ""));

    let run = check(&server, Ecosystem::Cargo, &many(2)).await;

    assert_eq!(
        run.answers
            .values()
            .map(PackageAnswer::failure)
            .collect::<Vec<_>>(),
        [
            Some(&SourceFailure::Status { code: 404 }),
            Some(&SourceFailure::Status { code: 404 })
        ]
    );
}

#[rstest::rstest]
#[case::another_id(Reply::json(200, &record("OTHER-1")))]
#[case::not_found(Reply::new(404, ""))]
#[tokio::test]
async fn a_record_that_is_not_the_one_asked_for_leaves_an_id_only_advisory(#[case] reply: Reply) {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, batch(vec![with_vulns(&["X-1"])]));
    server.route("GET", "/v1/vulns/X-1", reply);

    let run = check(&server, Ecosystem::Cargo, &many(1)).await;

    let answer = &run.answers[&PackageKey(0)];
    assert!(matches!(answer, PackageAnswer::Answered { .. }));
    assert_eq!(
        answer.advisories(),
        [SourceAdvisory::id_only(SourceName::Osv, "X-1")]
    );
    assert_eq!(run.details_missing, 1);
}

#[rstest::rstest]
#[case::dot_dot("..")]
#[case::dot(".")]
#[case::empty("")]
#[tokio::test]
async fn an_id_that_would_be_a_dot_segment_is_never_fetched(#[case] id: &str) {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, batch(vec![with_vulns(&[id])]));

    let run = check(&server, Ecosystem::Cargo, &many(1)).await;

    let paths: Vec<String> = server.received().into_iter().map(|r| r.path).collect();
    assert_eq!(paths, [QUERYBATCH]);
    assert_eq!(ids_of(&run.answers[&PackageKey(0)]), [id]);
    assert_eq!(run.details_missing, 1);
}

#[tokio::test]
async fn a_record_id_is_encoded_as_one_segment() {
    let server = MockServer::start();
    server.route("POST", QUERYBATCH, batch(vec![with_vulns(&["A/B?c#d"])]));
    server.route(
        "GET",
        "/v1/vulns/A%2FB%3Fc%23d",
        Reply::json(200, &record("A/B?c#d")),
    );

    let run = check(&server, Ecosystem::Cargo, &many(1)).await;

    assert_eq!(server.received_at("/v1/vulns/A%2FB%3Fc%23d").len(), 1);
    assert_eq!(run.details_missing, 0);
}

#[tokio::test]
#[ignore = "asks the real OSV.dev; run with --ignored live_"]
async fn live_osv_scoped_npm() {
    let client = client_for(Url::parse(OSV_BASE).unwrap());

    let run = client
        .check(
            Ecosystem::Npm,
            &[resolved(0, "@babel/traverse", "7.22.0")],
            &context(),
        )
        .await;

    let answer = &run.answers[&PackageKey(0)];
    assert!(
        matches!(answer, PackageAnswer::Answered { .. }),
        "{answer:?}"
    );
    assert!(!answer.advisories().is_empty());
    assert_eq!(run.details_missing, 0);
}

#[tokio::test]
async fn a_block_on_record_fetches_stops_them_and_leaves_ids_only() {
    let server = MockServer::start();
    let ids: Vec<String> = (0..10).map(|index| format!("X-{index}")).collect();
    let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    server.route("POST", QUERYBATCH, batch(vec![with_vulns(&id_refs)]));
    for id in &ids {
        server.route("GET", &format!("/v1/vulns/{id}"), Reply::new(429, ""));
    }

    let run = check(&server, Ecosystem::Cargo, &many(1)).await;

    let fetches = server.received().len() - 1;
    assert!(fetches <= RECORDS_IN_FLIGHT, "{fetches} record fetches");
    assert!(run.block.is_some());
    let answer = &run.answers[&PackageKey(0)];
    assert!(matches!(answer, PackageAnswer::Answered { .. }));
    assert_eq!(ids_of(answer).len(), 10);
    assert_eq!(run.details_missing, 10);
}
