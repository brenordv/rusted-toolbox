//! Next pages through the `Link` header, and the links refused.

use super::*;

#[rstest]
#[case::next_first(
    r#"<https://api.github.com/a?x=1,2>; rel="next", <https://api.github.com/b>; rel="prev""#,
    Some("https://api.github.com/a?x=1,2")
)]
#[case::next_middle(r#"<https://h/p>; rel="prev", <https://h/n?affects=a%40b,c%40d>; rel="next", <https://h/l>; rel="last""#, Some("https://h/n?affects=a%40b,c%40d"))]
#[case::next_last(
    r#"<https://h/p>; rel="prev", <https://h/n>; rel="next""#,
    Some("https://h/n")
)]
#[case::extra_params(r#"<https://h/n>; title="x"; rel="next""#, Some("https://h/n"))]
#[case::unquoted(r#"<https://h/n>; rel=next"#, Some("https://h/n"))]
#[case::upper_case(r#"<https://h/n>; REL="Next""#, Some("https://h/n"))]
#[case::spaced(r#"<https://h/n> ; rel = "next""#, Some("https://h/n"))]
#[case::several_values(r#"<https://h/n>; rel="next last""#, Some("https://h/n"))]
#[case::no_next(r#"<https://h/p>; rel="prev""#, None)]
#[case::nextish(r#"<https://h/p>; rel="nextpage""#, None)]
#[case::empty("", None)]
fn next_link_finds_the_rel_next_url(#[case] header: &str, #[case] expected: Option<&str>) {
    assert_eq!(next_link(header), Ok(expected));
}

#[test]
fn a_link_left_open_is_unreadable() {
    assert_eq!(
        next_link(r#"<https://h/p>; rel="prev", <https://h/n; rel="next""#),
        Err(SourceFailure::BadAnswer {
            detail: "a Link header could not be read".to_owned()
        })
    );
}

#[tokio::test]
async fn pages_are_followed_through_the_link_header() {
    let server = MockServer::start();
    let next = server.base().join("advisories?after=Y3Vyc29y").unwrap();
    server.route(
        "GET",
        ADVISORIES,
        page(vec![advisory("GHSA-a", "reviewed", &["tokio"])])
            .with_header("link", &format!("<{next}>; rel=\"next\"")),
    );
    server.route(
        "GET",
        ADVISORIES,
        page(vec![advisory("GHSA-b", "reviewed", &["tokio"])]),
    );
    server.route("GET", ADVISORIES, empty_page());

    let run = check(&server, Ecosystem::Cargo, &[resolved(0, "tokio", "1.20.0")]).await;

    assert_eq!(queries(&server)[1], "after=Y3Vyc29y");
    let ids: Vec<&str> = run.answers[&PackageKey(0)]
        .advisories()
        .iter()
        .map(|advisory| advisory.id.as_str())
        .collect();
    assert_eq!(ids, ["GHSA-a", "GHSA-b"]);
}

#[tokio::test]
async fn a_next_link_in_a_second_link_line_is_followed() {
    let server = MockServer::start();
    let last = server.base().join("advisories?before=bGFzdA").unwrap();
    let next = server.base().join("advisories?after=bmV4dA").unwrap();
    server.route(
        "GET",
        ADVISORIES,
        empty_page()
            .with_header("link", &format!("<{last}>; rel=\"last\""))
            .with_header("link", &format!("<{next}>; rel=\"next\"")),
    );
    server.route("GET", ADVISORIES, empty_page());

    check(&server, Ecosystem::Cargo, &[resolved(0, "tokio", "1.20.0")]).await;

    assert_eq!(queries(&server)[1], "after=bmV4dA");
}

#[tokio::test]
async fn a_relative_next_link_is_read_against_the_advisories_address() {
    let server = MockServer::start();
    server.route(
        "GET",
        ADVISORIES,
        empty_page().with_header("link", "</advisories?after=cmVs>; rel=\"next\""),
    );
    server.route("GET", ADVISORIES, empty_page());

    check(&server, Ecosystem::Cargo, &[resolved(0, "tokio", "1.20.0")]).await;

    assert_eq!(queries(&server)[1], "after=cmVs");
}

/// `HOST` in each link stands for the mock server's address, and `PORT` for its port.
#[rstest]
#[case::another_host(
    "http://localhost:PORT/advisories?page=2",
    "a next-page link pointed at another address"
)]
#[case::another_scheme(
    "https://HOST/advisories?page=2",
    "a next-page link pointed at another address"
)]
#[case::another_port(
    "http://127.0.0.1:1/advisories?page=2",
    "a next-page link pointed at another address"
)]
#[case::another_path(
    "http://HOST/users?page=2",
    "a next-page link pointed at another address"
)]
#[case::a_user_name(
    "http://someone@HOST/advisories?page=2",
    "a next-page link pointed at another address"
)]
#[case::a_password(
    "http://:secret@HOST/advisories?page=2",
    "a next-page link pointed at another address"
)]
#[case::not_an_address("http://[::1/advisories", "a next-page link was not an address")]
#[tokio::test]
async fn a_next_link_elsewhere_fails_the_chunk(#[case] link: &str, #[case] detail: &str) {
    let server = MockServer::start();
    let link = link
        .replace("HOST", &server.addr.to_string())
        .replace("PORT", &server.addr.port().to_string());
    server.route(
        "GET",
        ADVISORIES,
        empty_page().with_header("link", &format!("<{link}>; rel=\"next\"")),
    );

    let run = check(&server, Ecosystem::Cargo, &[resolved(0, "tokio", "1.20.0")]).await;

    assert_eq!(
        run.answers[&PackageKey(0)].failure(),
        Some(&SourceFailure::BadAnswer {
            detail: detail.to_owned()
        })
    );
    assert_eq!(server.received().len(), 1);
}

#[tokio::test]
async fn an_unreadable_link_header_fails_the_chunk() {
    let server = MockServer::start();
    server.route(
        "GET",
        ADVISORIES,
        empty_page().with_header("link", "<https://h/n; rel=\"next\""),
    );

    let run = check(&server, Ecosystem::Cargo, &[resolved(0, "tokio", "1.20.0")]).await;

    assert_eq!(
        run.answers[&PackageKey(0)].failure(),
        Some(&SourceFailure::BadAnswer {
            detail: "a Link header could not be read".to_owned()
        })
    );
}

#[tokio::test]
async fn more_than_ten_pages_fails_the_chunk() {
    let server = MockServer::start();
    let next = server.base().join("advisories?after=more").unwrap();
    server.route(
        "GET",
        ADVISORIES,
        empty_page().with_header("link", &format!("<{next}>; rel=\"next\"")),
    );

    let run = check(&server, Ecosystem::Cargo, &[resolved(0, "tokio", "1.20.0")]).await;

    assert_eq!(server.received().len(), 10);
    assert_eq!(
        run.answers[&PackageKey(0)].failure(),
        Some(&SourceFailure::BadAnswer {
            detail: "more than 10 pages".to_owned()
        })
    );
}
