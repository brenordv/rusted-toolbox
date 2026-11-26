use super::*;
use crate::engine::Clock;
use crate::test_support::{MockServer, Reply, closed_port, cut_short, fixed_clock, stepping_clock};
use chrono::TimeDelta;
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const AGENT: &str = "dep-check-test/1.0 (+https://example.invalid)";
const MARKER: &str = "marker-48151";

#[derive(Debug, PartialEq, Deserialize)]
struct Sample {
    name: String,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Plain,
}

#[derive(Debug, PartialEq, Deserialize)]
struct Tagged {
    kind: Kind,
}

fn client(limits: &HttpLimits) -> Client {
    build_client(limits, AGENT, HeaderMap::new()).unwrap()
}

async fn get_sample(
    base: &Url,
    limits: &HttpLimits,
    clock: &Clock,
) -> Result<Answer<Sample>, SourceFailure> {
    get_json(
        &client(limits),
        base.join("v1/thing?secret=1").unwrap(),
        limits,
        clock,
        "thing",
    )
    .await
}

fn now_clock() -> Clock {
    fixed_clock(Utc::now())
}

#[tokio::test]
async fn get_json_parses_a_200_and_sends_the_agent_it_was_built_with() {
    let server = MockServer::start();
    server.route(
        "GET",
        "/v1/thing",
        Reply::json(200, &serde_json::json!({"name": "serde"})),
    );

    let answer = get_sample(&server.base(), &HttpLimits::default(), &now_clock()).await;

    assert_eq!(
        answer,
        Ok(Answer::Found(Sample {
            name: "serde".to_owned()
        }))
    );
    assert_eq!(server.received()[0].header("user-agent"), Some(AGENT));
}

#[tokio::test]
async fn get_json_reads_a_404_as_not_found() {
    let server = MockServer::start();
    server.route("GET", "/v1/thing", Reply::new(404, "no such thing"));

    let answer = get_sample(&server.base(), &HttpLimits::default(), &now_clock()).await;

    assert_eq!(answer, Ok(Answer::NotFound));
}

#[tokio::test]
async fn a_body_past_the_cap_is_a_bad_answer() {
    let server = MockServer::start();
    let body = vec![b' '; 17 * 1024 * 1024];
    server.route("GET", "/v1/thing", Reply::new(200, body));

    let answer = get_sample(&server.base(), &HttpLimits::default(), &now_clock()).await;

    assert_eq!(
        answer,
        Err(SourceFailure::BadAnswer {
            detail: "the answer was larger than 16 MiB".to_owned()
        })
    );
}

#[tokio::test]
async fn a_server_that_never_answers_times_out() {
    let server = MockServer::start();
    server.route("GET", "/v1/thing", Reply::hold());
    let limits = HttpLimits {
        request_timeout: Duration::from_millis(200),
        ..HttpLimits::default()
    };
    let started = Instant::now();

    let answer = get_sample(&server.base(), &limits, &now_clock()).await;

    assert_eq!(answer, Err(SourceFailure::TimedOut));
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn a_closed_port_is_unreachable_without_the_path_or_query() {
    let base = closed_port();

    let answer = get_sample(&base, &HttpLimits::default(), &now_clock()).await;

    let Err(SourceFailure::Unreachable { detail }) = answer else {
        panic!("not unreachable: {answer:?}");
    };
    assert!(!detail.contains("v1/thing"), "{detail}");
    assert!(!detail.contains("secret"), "{detail}");
}

#[tokio::test]
async fn a_429_blocks_from_when_the_answer_arrived() {
    let server = Arc::new(MockServer::start());
    server.route(
        "GET",
        "/v1/thing",
        Reply::new(429, "").with_header("retry-after", "120"),
    );
    let start = Utc::now();
    let requests_seen_at_each_read = Arc::new(Mutex::new(Vec::new()));
    let (watched, reads) = (Arc::clone(&server), Arc::clone(&requests_seen_at_each_read));
    let clock: Clock = Arc::new(move || {
        reads.lock().unwrap().push(watched.received().len());
        start
    });

    let answer = get_sample(&server.base(), &HttpLimits::default(), &clock).await;

    assert_eq!(*requests_seen_at_each_read.lock().unwrap(), [1]);
    assert_eq!(
        answer,
        Err(SourceFailure::RateLimited {
            until: start + TimeDelta::seconds(120)
        })
    );
}

#[test]
fn a_stepping_clock_moves_one_step_per_read() {
    let start = Utc::now();
    let clock = stepping_clock(start, TimeDelta::minutes(1));

    let reads = [clock(), clock(), clock()];

    assert_eq!(
        reads,
        [
            start,
            start + TimeDelta::minutes(1),
            start + TimeDelta::minutes(2)
        ]
    );
}

#[rstest::rstest]
#[case::no_header(None, TimeDelta::seconds(60))]
#[case::http_date(Some("Wed, 21 Oct 2026 07:28:00 GMT"), TimeDelta::seconds(60))]
#[case::huge(Some("18446744073709551615"), TimeDelta::seconds(3600))]
#[case::past_u64(Some("99999999999999999999999"), TimeDelta::seconds(3600))]
#[tokio::test]
async fn a_429_without_a_usable_retry_after_waits_a_minute_and_a_huge_one_an_hour(
    #[case] retry_after: Option<&str>,
    #[case] wait: TimeDelta,
) {
    let server = MockServer::start();
    let reply = match retry_after {
        Some(value) => Reply::new(429, "").with_header("retry-after", value),
        None => Reply::new(429, ""),
    };
    server.route("GET", "/v1/thing", reply);
    let now = Utc::now();

    let answer = get_sample(&server.base(), &HttpLimits::default(), &fixed_clock(now)).await;

    assert_eq!(
        answer,
        Err(SourceFailure::RateLimited { until: now + wait })
    );
}

#[tokio::test]
async fn a_503_is_a_status() {
    let server = MockServer::start();
    server.route("GET", "/v1/thing", Reply::new(503, MARKER));

    let answer = get_sample(&server.base(), &HttpLimits::default(), &now_clock()).await;

    assert_eq!(answer, Err(SourceFailure::Status { code: 503 }));
}

#[tokio::test]
async fn a_redirect_is_a_status_and_is_not_followed() {
    let server = MockServer::start();
    let target = server.base().join("v1/elsewhere").unwrap();
    server.route(
        "GET",
        "/v1/thing",
        Reply::new(302, "").with_header("location", target.as_str()),
    );

    let answer = get_sample(&server.base(), &HttpLimits::default(), &now_clock()).await;

    assert_eq!(answer, Err(SourceFailure::Status { code: 302 }));
    assert!(server.received_at("/v1/elsewhere").is_empty());
}

#[tokio::test]
async fn malformed_json_is_a_bad_answer_with_a_position_and_no_answer_text() {
    let server = MockServer::start();
    server.route(
        "GET",
        "/v1/thing",
        Reply::json(200, &serde_json::json!({"kind": MARKER})),
    );
    let limits = HttpLimits::default();

    let answer: Result<Answer<Tagged>, SourceFailure> = get_json(
        &client(&limits),
        server.base().join("v1/thing").unwrap(),
        &limits,
        &now_clock(),
        "thing",
    )
    .await;

    let Err(SourceFailure::BadAnswer { detail }) = answer else {
        panic!("not a bad answer: {answer:?}");
    };
    assert!(
        detail.starts_with("a data error at line 1, column "),
        "{detail}"
    );
    assert!(!detail.contains(MARKER), "{detail}");
}

#[tokio::test]
async fn a_body_cut_short_is_interrupted() {
    let base = cut_short(b"{\"name\":");

    let answer = get_sample(&base, &HttpLimits::default(), &now_clock()).await;

    assert_eq!(answer, Err(SourceFailure::Interrupted));
}

fn unreachable() -> SourceFailure {
    SourceFailure::Unreachable {
        detail: "refused".to_owned(),
    }
}

#[test]
fn three_transport_failures_in_a_row_stop_the_source() {
    let gate = SourceGate::default();
    let failed: Result<(), SourceFailure> = Err(unreachable());

    for _ in 0..STOP_AFTER_FAILURES {
        gate.open().unwrap();
        gate.record(&failed);
    }

    assert_eq!(gate.open(), Err(unreachable()));
    assert_eq!(gate.requests(), STOP_AFTER_FAILURES);
}

#[test]
fn an_answer_in_flight_after_the_stop_does_not_reopen_the_source() {
    let gate = SourceGate::default();
    let failed: Result<(), SourceFailure> = Err(SourceFailure::TimedOut);
    let answered: Result<(), SourceFailure> = Ok(());

    gate.record(&failed);
    gate.record(&failed);
    gate.record(&failed);
    gate.record(&answered);

    assert_eq!(gate.open(), Err(SourceFailure::TimedOut));
}

#[test]
fn an_answer_between_failures_resets_the_count() {
    let gate = SourceGate::default();
    let failed: Result<(), SourceFailure> = Err(SourceFailure::TimedOut);
    let status: Result<(), SourceFailure> = Err(SourceFailure::Status { code: 500 });

    gate.record(&failed);
    gate.record(&failed);
    gate.record(&status);
    gate.record(&failed);
    gate.record(&failed);

    assert_eq!(gate.open(), Ok(()));
}

#[test]
fn a_block_stops_the_source_and_the_later_block_is_kept() {
    let gate = SourceGate::default();
    let now = Utc::now();
    let later: Result<(), SourceFailure> = Err(SourceFailure::RateLimited {
        until: now + TimeDelta::minutes(5),
    });
    let sooner: Result<(), SourceFailure> = Err(SourceFailure::RateLimited {
        until: now + TimeDelta::minutes(1),
    });

    gate.record(&later);
    gate.record(&sooner);

    assert_eq!(gate.block(), Some(now + TimeDelta::minutes(5)));
    assert_eq!(
        gate.open(),
        Err(SourceFailure::RateLimited {
            until: now + TimeDelta::minutes(5)
        })
    );
}

#[test]
fn encode_segment_encodes_reserved_characters_and_keeps_unreserved_ones() {
    let encoded = encode_segment("a-b.c_d~e/f@g:h+i!j%k Z9");

    assert_eq!(encoded, "a-b.c_d~e%2Ff%40g%3Ah%2Bi%21j%25k%20Z9");
}

#[rstest::rstest]
#[case::unreachable(unreachable(), "could not be reached: refused")]
#[case::timed_out(SourceFailure::TimedOut, "did not answer in time")]
#[case::interrupted(
    SourceFailure::Interrupted,
    "lost the connection before the answer was complete"
)]
#[case::status(SourceFailure::Status { code: 503 }, "answered 503")]
#[case::bad_answer(
    SourceFailure::BadAnswer { detail: "more than 20 pages".to_owned() },
    "sent an answer the tool could not read: more than 20 pages"
)]
fn describe_gives_one_sentence_per_failure(#[case] failure: SourceFailure, #[case] text: &str) {
    assert_eq!(failure.describe(), text);
}

#[test]
fn describe_a_block_names_its_end() {
    let until = DateTime::parse_from_rfc3339("2026-11-26T14:00:05Z")
        .unwrap()
        .with_timezone(&Utc);

    let text = SourceFailure::RateLimited { until }.describe();

    assert_eq!(text, "is rate limited until 2026-11-26 14:00:05 UTC");
}
