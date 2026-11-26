use super::*;
use crate::advisory::AdvisoryKind;
use crate::http::{STOP_AFTER_FAILURES, build_client};
use crate::test_support::{MockServer, Reply, fixed_clock};
use reqwest::header::HeaderMap;
use rstest::rstest;
use serde_json::{Value, json};
use std::time::Duration;

fn client_for(base: Url) -> DepsDevClient {
    client_with(base, HttpLimits::default())
}

fn client_with(base: Url, limits: HttpLimits) -> DepsDevClient {
    let client = build_client(&limits, "dep-check-test/1.0", HeaderMap::new()).unwrap();
    DepsDevClient::new(client, base, limits)
}

fn context() -> RunContext {
    RunContext {
        clock: fixed_clock(Utc::now()),
        strikes: 0,
    }
}

fn resolved(index: usize, name: &str, version: &str) -> Resolved {
    Resolved {
        key: PackageKey(index),
        name: name.to_owned(),
        version: version.to_owned(),
    }
}

fn version_path(name: &str, version: &str) -> String {
    format!("/v3/systems/cargo/packages/{name}/versions/{version}")
}

fn many(count: usize) -> Vec<Resolved> {
    (0..count)
        .map(|index| resolved(index, &format!("crate{index}"), "1.0.0"))
        .collect()
}

async fn check(
    server: &MockServer,
    packages: &[Resolved],
) -> (SourceRun, BTreeMap<PackageKey, SupplyChain>) {
    client_for(server.base())
        .check(Ecosystem::Cargo, packages, LatestRun::default(), &context())
        .await
}

fn version_info(body: Value) -> Reply {
    Reply::json(200, &body)
}

#[rstest]
#[case::plain(
    Ecosystem::Cargo,
    "tokio",
    "1.20.0",
    "/v3/systems/cargo/packages/tokio/versions/1.20.0"
)]
#[case::scoped_npm(
    Ecosystem::Npm,
    "@types/node",
    "20.1.0",
    "/v3/systems/npm/packages/%40types%2Fnode/versions/20.1.0"
)]
#[case::pypi_as_given(
    Ecosystem::Pypi,
    "Foo_Bar",
    "1.0",
    "/v3/systems/pypi/packages/Foo_Bar/versions/1.0"
)]
#[case::nuget_as_given(
    Ecosystem::Nuget,
    "Newtonsoft.Json",
    "13.0.3",
    "/v3/systems/nuget/packages/Newtonsoft.Json/versions/13.0.3"
)]
#[case::build_metadata(
    Ecosystem::Cargo,
    "serde",
    "1.0.0+build",
    "/v3/systems/cargo/packages/serde/versions/1.0.0%2Bbuild"
)]
#[tokio::test]
async fn get_version_paths_encode_each_value_as_one_segment(
    #[case] ecosystem: Ecosystem,
    #[case] name: &str,
    #[case] version: &str,
    #[case] path: &str,
) {
    let server = MockServer::start();
    server.route("GET", path, version_info(json!({})));

    let (run, _) = client_for(server.base())
        .check(
            ecosystem,
            &[resolved(0, name, version)],
            LatestRun::default(),
            &context(),
        )
        .await;

    let paths: Vec<String> = server.received().into_iter().map(|r| r.path).collect();
    assert_eq!(paths, [path]);
    assert!(!paths[0].contains("//"));
    assert!(matches!(
        run.answers[&PackageKey(0)],
        PackageAnswer::Answered { .. }
    ));
}

async fn latest_of(reply: Reply) -> (Latest, usize) {
    let server = MockServer::start();
    server.route("GET", "/v3/systems/cargo/packages/serde", reply);

    let run = client_for(server.base())
        .latest_versions(
            Ecosystem::Cargo,
            &[(PackageKey(0), "serde".to_owned())],
            &context(),
        )
        .await;

    (run.answers[&PackageKey(0)].clone(), server.received().len())
}

fn versions(entries: &[(&str, bool)]) -> Reply {
    let versions: Vec<Value> = entries
        .iter()
        .map(|(version, is_default)| {
            json!({"versionKey": {"system": "CARGO", "name": "serde", "version": version}, "isDefault": is_default})
        })
        .collect();
    Reply::json(200, &json!({ "versions": versions }))
}

#[tokio::test]
async fn get_package_picks_the_default_version() {
    let (latest, _) = latest_of(versions(&[
        ("1.0.213", false),
        ("1.0.215", true),
        ("2.0.0-alpha", false),
    ]))
    .await;

    assert_eq!(latest, Latest::Version("1.0.215".to_owned()));
}

#[tokio::test]
async fn get_package_with_no_default_names_none() {
    let (latest, _) = latest_of(versions(&[("1.0.0", false)])).await;

    assert_eq!(
        latest,
        Latest::Failed(SourceFailure::BadAnswer {
            detail: "deps.dev names no default version".to_owned()
        })
    );
}

#[rstest]
#[case::not_found(Reply::new(404, ""), Latest::NotFound)]
#[case::unavailable(
    Reply::new(503, ""),
    Latest::Failed(SourceFailure::Status { code: 503 })
)]
#[tokio::test]
async fn get_package_status_answers(#[case] reply: Reply, #[case] expected: Latest) {
    let (latest, _) = latest_of(reply).await;

    assert_eq!(latest, expected);
}

#[rstest]
#[case::path("1.0/../x")]
#[case::dot_dot("..")]
#[tokio::test]
async fn a_default_version_the_rule_refuses_fails_and_asks_nothing_more(#[case] version: &str) {
    let (latest, requests) = latest_of(versions(&[(version, true)])).await;

    assert_eq!(
        latest,
        Latest::Failed(SourceFailure::BadAnswer {
            detail: "deps.dev named a latest version this tool does not accept".to_owned()
        })
    );
    assert_eq!(requests, 1);
}

#[tokio::test]
async fn get_version_gives_the_supply_chain() {
    let server = MockServer::start();
    server.route(
        "GET",
        &version_path("a", "1.0.0"),
        version_info(json!({
            "isDefault": true,
            "isDeprecated": true,
            "licenses": ["MIT", "non-standard"],
            "publishedAt": "2024-11-11T00:00:00Z"
        })),
    );
    server.route(
        "GET",
        &version_path("b", "1.0.0"),
        version_info(json!({"isDeprecated": true, "deprecatedReason": "use c instead"})),
    );
    server.route("GET", &version_path("c", "1.0.0"), version_info(json!({})));

    let (_, chains) = check(
        &server,
        &[
            resolved(0, "a", "1.0.0"),
            resolved(1, "b", "1.0.0"),
            resolved(2, "c", "1.0.0"),
        ],
    )
    .await;

    let a = &chains[&PackageKey(0)];
    assert_eq!(
        (a.latest, a.deprecated.as_deref(), a.licenses.as_slice()),
        (
            true,
            Some(""),
            ["MIT".to_owned(), "non-standard".to_owned()].as_slice()
        )
    );
    assert!(a.published_at.is_some());
    assert_eq!(
        chains[&PackageKey(1)].deprecated.as_deref(),
        Some("use c instead")
    );
    assert_eq!(
        (
            chains[&PackageKey(2)].latest,
            chains[&PackageKey(2)].deprecated.as_deref()
        ),
        (false, None)
    );
}

#[tokio::test]
async fn get_version_404_is_not_found_with_no_supply_chain() {
    let server = MockServer::start();
    server.route("GET", &version_path("crate0", "1.0.0"), Reply::new(404, ""));

    let (run, chains) = check(&server, &many(1)).await;

    assert_eq!(run.answers[&PackageKey(0)], PackageAnswer::NotFound);
    assert!(chains.is_empty());
}

fn with_repo(id: &str) -> Value {
    json!({"relatedProjects": [
        {"projectKey": {"id": "github.com/someone/issues"}, "relationType": "ISSUE_TRACKER"},
        {"projectKey": {"id": id}, "relationType": "SOURCE_REPO", "relationProvenance": "UNVERIFIED_METADATA"}
    ]})
}

#[tokio::test]
async fn one_project_request_per_distinct_source_repo() {
    let server = MockServer::start();
    for index in 0..3 {
        server.route(
            "GET",
            &version_path(&format!("crate{index}"), "1.0.0"),
            version_info(with_repo("github.com/org/repo")),
        );
    }
    server.route(
        "GET",
        "/v3/projects/github.com%2Forg%2Frepo",
        Reply::json(
            200,
            &json!({"scorecard": {"date": "2026-11-17T00:00:00Z", "overallScore": 8.1}}),
        ),
    );

    let (_, chains) = check(&server, &many(3)).await;

    assert_eq!(
        server
            .received_at("/v3/projects/github.com%2Forg%2Frepo")
            .len(),
        1
    );
    assert!(
        server
            .received_at("/v3/projects/github.com%2Fsomeone%2Fissues")
            .is_empty()
    );
    let chain = &chains[&PackageKey(2)];
    assert_eq!(
        chain.source_repo,
        Some(SourceRepo {
            id: "github.com/org/repo".to_owned(),
            provenance: "unverified_metadata".to_owned()
        })
    );
    assert_eq!(chain.scorecard.as_ref().map(|card| card.score), Some(8.1));
}

#[tokio::test]
async fn a_failed_project_leaves_no_scorecard_and_the_package_answered() {
    let server = MockServer::start();
    server.route(
        "GET",
        &version_path("crate0", "1.0.0"),
        version_info(with_repo("github.com/org/repo")),
    );
    server.route(
        "GET",
        "/v3/projects/github.com%2Forg%2Frepo",
        Reply::new(500, ""),
    );

    let (run, chains) = check(&server, &many(1)).await;

    assert!(matches!(
        run.answers[&PackageKey(0)],
        PackageAnswer::Answered { .. }
    ));
    assert_eq!(chains[&PackageKey(0)].scorecard, None);
    assert_eq!(run.details_missing, 1);
}

#[tokio::test]
async fn a_project_deps_dev_does_not_know_is_not_a_missing_detail() {
    let server = MockServer::start();
    server.route(
        "GET",
        &version_path("crate0", "1.0.0"),
        version_info(with_repo("github.com/org/repo")),
    );
    server.route(
        "GET",
        "/v3/projects/github.com%2Forg%2Frepo",
        Reply::new(404, ""),
    );

    let (run, chains) = check(&server, &many(1)).await;

    assert_eq!(chains[&PackageKey(0)].scorecard, None);
    assert_eq!(run.details_missing, 0);
}

#[tokio::test]
async fn ids_from_an_answer_that_would_be_dot_segments_are_never_fetched() {
    let server = MockServer::start();
    server.route(
        "GET",
        &version_path("crate0", "1.0.0"),
        version_info(json!({
            "advisoryKeys": [{"id": ".."}],
            "relatedProjects": [{"projectKey": {"id": ".."}, "relationType": "SOURCE_REPO"}]
        })),
    );

    let (run, _) = check(&server, &many(1)).await;

    let paths: Vec<String> = server.received().into_iter().map(|r| r.path).collect();
    assert_eq!(paths, [version_path("crate0", "1.0.0")]);
    assert_eq!(
        run.answers[&PackageKey(0)].advisories(),
        [SourceAdvisory::id_only(SourceName::DepsDev, "..")]
    );
    assert_eq!(run.details_missing, 2);
}

#[tokio::test]
async fn attestations_count_both_lists_and_their_verified_flags() {
    let server = MockServer::start();
    server.route(
        "GET",
        &version_path("crate0", "1.0.0"),
        version_info(json!({
            "attestations": [{"type": "https://slsa.dev/provenance/v1", "verified": true}, {"verified": false}],
            "slsaProvenances": [{"sourceRepository": "x", "verified": true}]
        })),
    );

    let (_, chains) = check(&server, &many(1)).await;

    assert_eq!(
        chains[&PackageKey(0)].attestations,
        Attestations {
            total: 3,
            verified: 2
        }
    );
}

#[tokio::test]
async fn advisories_are_fetched_once_each_and_a_failed_one_stays_an_id() {
    let server = MockServer::start();
    let keys = json!({"advisoryKeys": [{"id": "GHSA-1"}, {"id": "GHSA-2"}]});
    server.route(
        "GET",
        &version_path("crate0", "1.0.0"),
        version_info(keys.clone()),
    );
    server.route("GET", &version_path("crate1", "1.0.0"), version_info(keys));
    server.route(
        "GET",
        "/v3/advisories/GHSA-1",
        Reply::json(
            200,
            &json!({
                "advisoryKey": {"id": "GHSA-1"},
                "url": "https://osv.dev/vulnerability/GHSA-1",
                "title": "a title",
                "aliases": ["CVE-1"],
                "cvss3Score": 7.5,
                "cvss3Vector": "CVSS:3.1/AV:N"
            }),
        ),
    );
    server.route("GET", "/v3/advisories/GHSA-2", Reply::new(503, ""));

    let (run, _) = check(&server, &many(2)).await;

    assert_eq!(server.received_at("/v3/advisories/GHSA-1").len(), 1);
    let advisories = run.answers[&PackageKey(1)].advisories();
    assert_eq!(
        advisories[0],
        SourceAdvisory {
            source: SourceName::DepsDev,
            id: "GHSA-1".to_owned(),
            aliases: vec!["CVE-1".to_owned()],
            kind: AdvisoryKind::Vulnerability,
            summary: Some("a title".to_owned()),
            severity: None,
            cvss: vec![Cvss {
                version: "3".to_owned(),
                vector: Some("CVSS:3.1/AV:N".to_owned()),
                score: Some(7.5),
                from: SourceName::DepsDev
            }],
            epss: None,
            fixed_in: Vec::new(),
            urls: vec!["https://osv.dev/vulnerability/GHSA-1".to_owned()],
            published: None,
        }
    );
    assert_eq!(
        advisories[1],
        SourceAdvisory::id_only(SourceName::DepsDev, "GHSA-2")
    );
    assert_eq!(run.details_missing, 1);
}

#[tokio::test]
async fn an_advisory_answer_about_another_id_stays_an_id() {
    let server = MockServer::start();
    server.route(
        "GET",
        &version_path("crate0", "1.0.0"),
        version_info(json!({"advisoryKeys": [{"id": "GHSA-1"}]})),
    );
    server.route(
        "GET",
        "/v3/advisories/GHSA-1",
        Reply::json(
            200,
            &json!({"advisoryKey": {"id": "GHSA-9"}, "title": "other"}),
        ),
    );

    let (run, _) = check(&server, &many(1)).await;

    assert_eq!(
        run.answers[&PackageKey(0)].advisories(),
        [SourceAdvisory::id_only(SourceName::DepsDev, "GHSA-1")]
    );
    assert_eq!(run.details_missing, 1);
}

/// GetAdvisory for RUSTSEC-2021-0145 as deps.dev answered it on 2026-11-26.
const LIVE_ADVISORY: &str = r#"{"advisoryKey":{"id":"RUSTSEC-2021-0145"},"url":"https://osv.dev/vulnerability/RUSTSEC-2021-0145","title":"Potential unaligned read","aliases":["GHSA-g98v-hv3f-hcfr"],"cvss3Score":0,"cvss3Vector":""}"#;

/// GetVersion for cargo serde 1.0.100 as deps.dev answered it on 2026-11-26.
const LIVE_VERSION: &str = r#"{"versionKey":{"system":"CARGO","name":"serde","version":"1.0.100"},"publishedAt":"2019-09-08T01:56:06Z","isDefault":false,"isDeprecated":false,"deprecatedReason":"","licenses":["Apache-2.0 OR MIT"],"advisoryKeys":[],"links":[{"label":"SOURCE_REPO","url":"https://github.com/serde-rs/serde"},{"label":"HOMEPAGE","url":"https://serde.rs"},{"label":"DOCUMENTATION","url":"https://docs.rs/serde"}],"slsaProvenances":[],"attestations":[],"registries":["https://crates.io/"],"relatedProjects":[{"projectKey":{"id":"github.com/serde-rs/serde"},"relationProvenance":"UNVERIFIED_METADATA","relationType":"SOURCE_REPO"}]}"#;

/// GetProject for github.com/serde-rs/serde as deps.dev answered it on 2026-11-26, its
/// `checks` cut to the first.
const LIVE_PROJECT: &str = r#"{"projectKey":{"id":"github.com/serde-rs/serde"},"openIssuesCount":411,"starsCount":10971,"forksCount":979,"license":"Apache-2.0","description":"Serialization framework for Rust","homepage":"https://serde.rs/","scorecard":{"date":"2026-08-24T00:00:00Z","repository":{"name":"github.com/serde-rs/serde","commit":"a874a1b1bb1cc16cf5ee3b1b7b527af5705742bb"},"scorecard":{"version":"v5.5.1-0.20260815060127-d1fab88f5463","commit":"d1fab88f54636ff366076edfc5c239f97b3c8e66"},"checks":[{"name":"Code-Review","documentation":{"shortDescription":"Determines if the project requires human code review before pull requests (aka merge requests) are merged.","url":"https://github.com/ossf/scorecard/blob/d1fab88f54636ff366076edfc5c239f97b3c8e66/docs/checks.md#code-review"},"score":2,"reason":"Found 6/22 approved changesets -- score normalized to 2","details":[]}],"overallScore":5.6,"metadata":[]}}"#;

#[test]
fn a_live_advisory_with_no_cvss_3_rating_has_no_rating() {
    let info: AdvisoryInfo = serde_json::from_str(LIVE_ADVISORY).unwrap();

    let advisory = advisory_from_info("RUSTSEC-2021-0145", info);

    assert_eq!(advisory.cvss, Vec::new());
    assert_eq!(
        advisory.summary.as_deref(),
        Some("Potential unaligned read")
    );
    assert_eq!(advisory.aliases, ["GHSA-g98v-hv3f-hcfr"]);
    assert_eq!(
        advisory.urls,
        ["https://osv.dev/vulnerability/RUSTSEC-2021-0145"]
    );
}

#[test]
fn empty_texts_from_deps_dev_read_as_absent() {
    let info: AdvisoryInfo =
        serde_json::from_value(json!({"url": "", "title": "", "aliases": [""]})).unwrap();

    let advisory = advisory_from_info("X-1", info);

    assert_eq!(
        (advisory.summary, advisory.urls, advisory.aliases),
        (None, Vec::new(), Vec::new())
    );
}

#[test]
fn a_zero_score_with_its_vector_is_a_rating() {
    let info: AdvisoryInfo = serde_json::from_value(
        json!({"cvss3Score": 0, "cvss3Vector": "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:N"}),
    )
    .unwrap();

    let advisory = advisory_from_info("X-1", info);

    assert_eq!(advisory.cvss[0].score, Some(0.0));
}

#[tokio::test]
async fn live_version_and_project_answers_give_their_supply_chain() {
    let server = MockServer::start();
    server.route(
        "GET",
        "/v3/systems/cargo/packages/serde/versions/1.0.100",
        Reply::new(200, LIVE_VERSION),
    );
    server.route(
        "GET",
        "/v3/projects/github.com%2Fserde-rs%2Fserde",
        Reply::new(200, LIVE_PROJECT),
    );

    let (run, chains) = check(&server, &[resolved(0, "serde", "1.0.100")]).await;

    let chain = &chains[&PackageKey(0)];
    assert_eq!(
        chain,
        &SupplyChain {
            latest: false,
            deprecated: None,
            licenses: vec!["Apache-2.0 OR MIT".to_owned()],
            published_at: Some("2019-09-08T01:56:06Z".parse().unwrap()),
            source_repo: Some(SourceRepo {
                id: "github.com/serde-rs/serde".to_owned(),
                provenance: "unverified_metadata".to_owned()
            }),
            scorecard: Some(ScorecardSummary {
                score: 5.6,
                date: Some("2026-08-24T00:00:00Z".parse().unwrap())
            }),
            attestations: Attestations::default(),
        }
    );
    assert_eq!(run.details_missing, 0);
}

#[tokio::test]
async fn a_429_on_a_version_leaves_every_later_package_unsent() {
    let server = MockServer::start();
    for index in 0..12 {
        let reply = if index == 2 {
            Reply::new(429, "").with_header("retry-after", "30")
        } else {
            version_info(json!({}))
        };
        server.route(
            "GET",
            &version_path(&format!("crate{index}"), "1.0.0"),
            reply,
        );
    }

    let (run, _) = check(&server, &many(12)).await;

    let paths: Vec<String> = server.received().into_iter().map(|r| r.path).collect();
    assert!(run.block.is_some());
    assert!(matches!(
        run.answers[&PackageKey(2)].failure(),
        Some(SourceFailure::RateLimited { .. })
    ));
    for (key, answer) in &run.answers {
        let blocked = matches!(answer.failure(), Some(SourceFailure::RateLimited { .. }));
        let path = version_path(&format!("crate{}", key.0), "1.0.0");
        assert!(
            !blocked || key.0 == 2 || !paths.contains(&path),
            "{path} was sent and still reads as blocked"
        );
    }
}

#[tokio::test]
async fn failures_in_a_row_during_lookups_stop_the_version_requests() {
    let server = MockServer::start();
    for index in 0..3 {
        server.route(
            "GET",
            &format!("/v3/systems/cargo/packages/new{index}"),
            Reply::hold(),
        );
    }
    let client = client_with(
        server.base(),
        HttpLimits {
            request_timeout: Duration::from_millis(200),
            ..HttpLimits::default()
        },
    );
    let context = context();
    let lookups: Vec<(PackageKey, String)> = (0..3)
        .map(|index| (PackageKey(index), format!("new{index}")))
        .collect();

    let latest = client
        .latest_versions(Ecosystem::Cargo, &lookups, &context)
        .await;
    let (run, _) = client
        .check(
            Ecosystem::Cargo,
            &[resolved(3, "tokio", "1.0.0"), resolved(4, "serde", "1.0.0")],
            latest,
            &context,
        )
        .await;

    assert_eq!(server.received().len(), 3);
    assert_eq!(run.requests, 3);
    assert!(
        run.answers
            .values()
            .all(|answer| answer.failure() == Some(&SourceFailure::TimedOut))
    );
}

#[tokio::test]
async fn a_429_on_a_lookup_sends_no_version_request() {
    let server = MockServer::start();
    server.route(
        "GET",
        "/v3/systems/cargo/packages/serde",
        Reply::new(429, "").with_header("retry-after", "30"),
    );
    let client = client_for(server.base());
    let context = context();

    let latest = client
        .latest_versions(
            Ecosystem::Cargo,
            &[(PackageKey(0), "serde".to_owned())],
            &context,
        )
        .await;
    let (run, _) = client
        .check(
            Ecosystem::Cargo,
            &[resolved(1, "tokio", "1.0.0")],
            latest,
            &context,
        )
        .await;

    assert_eq!(server.received().len(), 1);
    assert!(run.block.is_some());
    assert_eq!(run.requests, 1);
    assert!(matches!(
        run.answers[&PackageKey(0)].failure(),
        Some(SourceFailure::RateLimited { .. })
    ));
    assert!(matches!(
        run.answers[&PackageKey(1)].failure(),
        Some(SourceFailure::RateLimited { .. })
    ));
}

#[tokio::test]
async fn a_429_on_an_advisory_keeps_the_packages_answered_with_ids() {
    let server = MockServer::start();
    server.route(
        "GET",
        &version_path("crate0", "1.0.0"),
        version_info(json!({"advisoryKeys": [{"id": "GHSA-1"}]})),
    );
    server.route("GET", "/v3/advisories/GHSA-1", Reply::new(429, ""));

    let (run, _) = check(&server, &many(1)).await;

    let answer = &run.answers[&PackageKey(0)];
    assert!(matches!(answer, PackageAnswer::Answered { .. }));
    assert_eq!(
        answer.advisories(),
        [SourceAdvisory::id_only(SourceName::DepsDev, "GHSA-1")]
    );
    assert!(run.block.is_some());
}

#[tokio::test]
async fn three_timeouts_in_a_row_stop_the_source() {
    let server = MockServer::start();
    for index in 0..10 {
        server.route(
            "GET",
            &version_path(&format!("crate{index}"), "1.0.0"),
            Reply::hold(),
        );
    }
    let limits = HttpLimits {
        request_timeout: Duration::from_millis(200),
        ..HttpLimits::default()
    };

    let (run, _) = client_with(server.base(), limits)
        .check(
            Ecosystem::Cargo,
            &many(10),
            LatestRun::default(),
            &context(),
        )
        .await;

    let most_sent = IN_FLIGHT + STOP_AFTER_FAILURES as usize - 1;
    assert!(
        server.received().len() <= most_sent,
        "{}",
        server.received().len()
    );
    assert!(
        run.answers
            .values()
            .all(|answer| answer.failure() == Some(&SourceFailure::TimedOut))
    );
}

#[tokio::test]
#[ignore = "asks the real deps.dev; run with --ignored live_"]
async fn live_deps_dev_not_found() {
    let client = client_for(Url::parse(DEPS_DEV_BASE).unwrap());
    let context = context();

    let latest = client
        .latest_versions(
            Ecosystem::Cargo,
            &[(PackageKey(0), "zz-no-such-crate-48151".to_owned())],
            &context,
        )
        .await;
    let (run, _) = client
        .check(
            Ecosystem::Cargo,
            &[resolved(1, "serde", "0.0.48151")],
            latest,
            &context,
        )
        .await;

    assert_eq!(run.answers[&PackageKey(0)], PackageAnswer::NotFound);
    assert_eq!(run.answers[&PackageKey(1)], PackageAnswer::NotFound);
}

#[tokio::test]
#[ignore = "asks the real deps.dev; run with --ignored live_"]
async fn live_deps_dev_project_scorecard() {
    let client = client_for(Url::parse(DEPS_DEV_BASE).unwrap());

    let (_, chains) = client
        .check(
            Ecosystem::Cargo,
            &[resolved(0, "tokio", "1.38.0")],
            LatestRun::default(),
            &context(),
        )
        .await;

    let chain = &chains[&PackageKey(0)];
    assert_eq!(
        chain.source_repo.as_ref().map(|repo| repo.id.as_str()),
        Some("github.com/tokio-rs/tokio")
    );
    assert!(chain.scorecard.is_some());
}
