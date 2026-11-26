//! Advisories that name several packages of a chunk, or none of them.

use super::*;

fn ids_of(run: &SourceRun, key: usize) -> Vec<String> {
    run.answers[&PackageKey(key)]
        .advisories()
        .iter()
        .map(|advisory| advisory.id.clone())
        .collect()
}

#[tokio::test]
async fn an_advisory_naming_two_packages_is_confirmed_for_each() {
    let server = MockServer::start();
    server.route(
        "GET",
        ADVISORIES,
        page(vec![advisory(
            "GHSA-two",
            "reviewed",
            &["tokio", "tokio-util"],
        )]),
    );
    server.route(
        "GET",
        ADVISORIES,
        page(vec![advisory("GHSA-two", "reviewed", &["tokio"])]),
    );
    server.route("GET", ADVISORIES, empty_page());

    let run = check(
        &server,
        Ecosystem::Cargo,
        &[
            resolved(0, "tokio", "1.20.0"),
            resolved(1, "tokio-util", "0.7.0"),
        ],
    )
    .await;

    let sent = queries(&server);
    assert_eq!(
        sent[1],
        "ecosystem=rust&type=reviewed&per_page=100&affects=tokio%401.20.0"
    );
    assert_eq!(
        sent[2],
        "ecosystem=rust&type=reviewed&per_page=100&affects=tokio-util%400.7.0"
    );
    assert_eq!(run.answers[&PackageKey(0)].advisories().len(), 1);
    assert!(run.answers[&PackageKey(1)].advisories().is_empty());
}

#[tokio::test]
async fn each_package_is_asked_alone_once_however_many_advisories_it_shares() {
    let server = MockServer::start();
    let shared = |id: &str| advisory(id, "reviewed", &["tokio", "tokio-util"]);
    server.route(
        "GET",
        ADVISORIES,
        page(vec![shared("GHSA-a"), shared("GHSA-b")]),
    );
    server.route(
        "GET",
        ADVISORIES,
        page(vec![shared("GHSA-a"), shared("GHSA-b")]),
    );
    server.route("GET", ADVISORIES, page(vec![shared("GHSA-b")]));
    server.route("GET", ADVISORIES, empty_page());

    let run = check(
        &server,
        Ecosystem::Cargo,
        &[
            resolved(0, "tokio", "1.20.0"),
            resolved(1, "tokio-util", "0.7.0"),
        ],
    )
    .await;

    assert_eq!(queries(&server).len(), 4);
    assert_eq!(ids_of(&run, 0), ["GHSA-a", "GHSA-b"]);
    assert_eq!(ids_of(&run, 1), ["GHSA-b"]);
    assert_eq!(run.requests, 4);
}

#[tokio::test]
async fn each_type_asks_its_packages_alone_with_its_own_type() {
    let server = MockServer::start();
    let names = ["tokio", "tokio-util"];
    server.route(
        "GET",
        ADVISORIES,
        page(vec![advisory("GHSA-r", "reviewed", &names)]),
    );
    server.route(
        "GET",
        ADVISORIES,
        page(vec![advisory("GHSA-r", "reviewed", &names)]),
    );
    server.route("GET", ADVISORIES, empty_page());
    server.route(
        "GET",
        ADVISORIES,
        page(vec![advisory("GHSA-m", "malware", &names)]),
    );
    server.route(
        "GET",
        ADVISORIES,
        page(vec![advisory("GHSA-m", "malware", &names)]),
    );
    server.route("GET", ADVISORIES, empty_page());

    let run = check(
        &server,
        Ecosystem::Cargo,
        &[
            resolved(0, "tokio", "1.20.0"),
            resolved(1, "tokio-util", "0.7.0"),
        ],
    )
    .await;

    let sent = queries(&server);
    assert_eq!(sent.len(), 6);
    assert_eq!(
        sent[4],
        "ecosystem=rust&type=malware&per_page=100&affects=tokio%401.20.0"
    );
    assert_eq!(ids_of(&run, 0), ["GHSA-r", "GHSA-m"]);
    assert!(ids_of(&run, 1).is_empty());
}

#[tokio::test]
async fn a_lone_ask_that_fails_on_a_later_page_keeps_what_its_first_page_listed() {
    let server = MockServer::start();
    let next = server.base().join("advisories?after=bW9yZQ").unwrap();
    let shared = advisory("GHSA-two", "reviewed", &["tokio", "tokio-util"]);
    server.route("GET", ADVISORIES, page(vec![shared.clone()]));
    server.route(
        "GET",
        ADVISORIES,
        page(vec![shared]).with_header("link", &format!("<{next}>; rel=\"next\"")),
    );
    server.route(
        "GET",
        ADVISORIES,
        Reply::new(403, "").with_header("retry-after", "30"),
    );

    let run = check(
        &server,
        Ecosystem::Cargo,
        &[
            resolved(0, "tokio", "1.20.0"),
            resolved(1, "tokio-util", "0.7.0"),
        ],
    )
    .await;

    let tokio = &run.answers[&PackageKey(0)];
    assert!(
        matches!(tokio.failure(), Some(SourceFailure::RateLimited { .. })),
        "{tokio:?}"
    );
    assert_eq!(ids_of(&run, 0), ["GHSA-two"]);
    assert_eq!(queries(&server).len(), 3);
}

#[tokio::test]
async fn an_advisory_naming_none_keeps_the_advisories_around_it_and_asks_malware() {
    let server = MockServer::start();
    server.route(
        "GET",
        ADVISORIES,
        page(vec![
            advisory("GHSA-a", "reviewed", &["tokio"]),
            advisory("GHSA-x", "reviewed", &["something-else"]),
            advisory("GHSA-b", "reviewed", &["serde"]),
        ]),
    );
    server.route("GET", ADVISORIES, empty_page());

    let run = check(
        &server,
        Ecosystem::Cargo,
        &[
            resolved(0, "tokio", "1.20.0"),
            resolved(1, "serde", "1.0.0"),
        ],
    )
    .await;

    let unnamed = SourceFailure::BadAnswer {
        detail: "an advisory named none of the packages asked".to_owned(),
    };
    for (key, id) in [(0, "GHSA-a"), (1, "GHSA-b")] {
        let answer = &run.answers[&PackageKey(key)];
        assert_eq!(answer.failure(), Some(&unnamed));
        assert_eq!(answer.advisories()[0].id, id);
    }
    assert_eq!(queries(&server).len(), 2);
}

#[tokio::test]
async fn a_blocked_confirm_fails_only_its_package() {
    let server = MockServer::start();
    server.route("GET", ADVISORIES, empty_page());
    server.route(
        "GET",
        ADVISORIES,
        page(vec![advisory(
            "GHSA-two",
            "malware",
            &["tokio", "tokio-util"],
        )]),
    );
    server.route(
        "GET",
        ADVISORIES,
        page(vec![advisory("GHSA-two", "malware", &["tokio"])]),
    );
    server.route(
        "GET",
        ADVISORIES,
        Reply::new(429, "").with_header("retry-after", "30"),
    );

    let run = check(
        &server,
        Ecosystem::Cargo,
        &[
            resolved(0, "tokio", "1.20.0"),
            resolved(1, "tokio-util", "0.7.0"),
        ],
    )
    .await;

    let first = &run.answers[&PackageKey(0)];
    assert!(matches!(first, PackageAnswer::Answered { .. }), "{first:?}");
    assert_eq!(first.advisories()[0].kind, AdvisoryKind::Malware);
    assert!(matches!(
        run.answers[&PackageKey(1)],
        PackageAnswer::Failed {
            failure: SourceFailure::RateLimited { .. },
            ..
        }
    ));
    assert!(run.answers[&PackageKey(1)].advisories().is_empty());
}

#[tokio::test]
async fn a_block_between_the_two_types_fails_every_package_and_keeps_what_was_found() {
    let server = MockServer::start();
    server.route(
        "GET",
        ADVISORIES,
        page(vec![advisory("GHSA-a", "reviewed", &["tokio"])]),
    );
    server.route(
        "GET",
        ADVISORIES,
        Reply::new(403, "").with_header("retry-after", "30"),
    );

    let run = check(
        &server,
        Ecosystem::Cargo,
        &[
            resolved(0, "tokio", "1.20.0"),
            resolved(1, "serde", "1.0.0"),
        ],
    )
    .await;

    assert!(
        run.answers
            .values()
            .all(|answer| answer.failure().is_some())
    );
    assert_eq!(run.answers[&PackageKey(0)].advisories()[0].id, "GHSA-a");
}
