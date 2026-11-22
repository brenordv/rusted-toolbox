use super::*;
use crate::test_support::{Listener, Reply, closed_port, cut_short_server};
use rstest::rstest;
use serde_json::json;

const SPOKEN: &str = "the words nobody else should read";

fn client() -> SpeechClient {
    SpeechClient::new().unwrap()
}

fn client_with(change: impl FnOnce(&mut Limits)) -> SpeechClient {
    let mut limits = Limits::default();
    change(&mut limits);
    SpeechClient::with_limits(limits).unwrap()
}

#[rstest]
#[case::post("POST")]
#[case::put("PUT")]
#[tokio::test]
async fn the_request_is_the_payload_plus_input_as_json_with_the_configured_method(
    #[case] method: &str,
) {
    let listener = Listener::start(Reply::audio(vec![1, 2, 3]));
    let mut config = listener.config();
    config.request_method = method.to_owned();
    config.payload.insert("input".to_owned(), json!("stale"));
    config.payload.insert("speed".to_owned(), json!(1.5));

    client().synthesize(&config, SPOKEN).await.unwrap();

    let received = listener.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].method, method);
    assert_eq!(
        received[0].content_type.as_deref(),
        Some("application/json")
    );
    assert_eq!(
        received[0].json(),
        json!({
            "model": "chatterbox-turbo",
            "voice": "sj-clip-001.wav",
            "speed": 1.5,
            "input": SPOKEN,
        })
    );
}

#[tokio::test]
async fn a_success_returns_the_bytes_and_the_content_type() {
    let listener = Listener::start(Reply::audio(vec![7, 8, 9]));

    let speech = client()
        .synthesize(&listener.config(), SPOKEN)
        .await
        .unwrap();

    assert_eq!(
        speech,
        Speech {
            bytes: vec![7, 8, 9],
            content_type: Some("audio/wav".to_owned()),
        }
    );
}

#[tokio::test]
async fn a_404_with_a_text_detail_is_an_error_holding_the_status_and_the_detail() {
    let listener = Listener::start(Reply::json(
        404,
        &json!({"detail": "Voice file 'nope.wav' not found."}),
    ));

    let error = client()
        .synthesize(&listener.config(), SPOKEN)
        .await
        .unwrap_err();

    let text = error.to_string();
    assert!(text.contains("404"), "{text}");
    assert!(text.contains("Voice file 'nope.wav' not found."), "{text}");
}

#[tokio::test]
async fn a_422_list_detail_gives_loc_and_msg_and_never_the_echoed_text() {
    let listener = Listener::start(Reply::json(
        422,
        &json!({"detail": [{
            "type": "missing",
            "loc": ["body", "voice"],
            "msg": "Field required",
            "input": {"model": "m", "input": SPOKEN},
        }]}),
    ));

    let error = client()
        .synthesize(&listener.config(), SPOKEN)
        .await
        .unwrap_err();

    let text = format!("{error:#}");
    assert!(text.contains("422"), "{text}");
    assert!(text.contains("body.voice: Field required"), "{text}");
    assert!(!text.contains(SPOKEN), "{text}");
}

#[tokio::test]
async fn a_500_with_a_long_text_body_is_cut_after_140_clean_characters() {
    let body = format!("Internal\u{1b}[31m error\r\n{}", "x".repeat(600));
    let listener = Listener::start(Reply::new(500, "text/plain", body));

    let error = client()
        .synthesize(&listener.config(), SPOKEN)
        .await
        .unwrap_err();

    let text = error.to_string();
    let detail = text.split_once("500 Internal Server Error: ").unwrap().1;
    assert_eq!(detail.chars().count(), 141);
    assert!(detail.ends_with('…'), "{detail}");
    assert!(!detail.chars().any(char::is_control), "{detail:?}");
}

#[tokio::test]
async fn an_error_body_is_read_no_further_than_its_cap() {
    let body = format!("{}{}", "A".repeat(10), "B".repeat(100_000));
    let listener = Listener::start(Reply::new(503, "text/plain", body));
    let client = client_with(|limits| limits.max_error_bytes = 10);

    let error = client
        .synthesize(&listener.config(), SPOKEN)
        .await
        .unwrap_err();

    let text = error.to_string();
    assert!(
        text.ends_with("503 Service Unavailable: AAAAAAAAAA"),
        "{text}"
    );
}

#[tokio::test]
async fn an_error_with_an_empty_body_carries_the_status_alone() {
    let listener = Listener::start(Reply::new(500, "text/plain", ""));

    let error = client()
        .synthesize(&listener.config(), SPOKEN)
        .await
        .unwrap_err();

    assert!(
        error
            .to_string()
            .ends_with("answered 500 Internal Server Error"),
        "{error:#}"
    );
}

#[tokio::test]
async fn a_refused_connection_names_host_and_port_and_neither_the_url_nor_the_text() {
    let port = closed_port();
    let config = SpeakConfig {
        url: format!("http://127.0.0.1:{port}/v1/audio/speech?session=private-query"),
        ..SpeakConfig::default()
    };

    let error = client().synthesize(&config, SPOKEN).await.unwrap_err();

    let text = format!("{error:#}");
    assert!(text.contains(&format!("127.0.0.1:{port}")), "{text}");
    assert!(text.contains("Could not connect"), "{text}");
    assert!(!text.contains("private-query"), "{text}");
    assert!(!text.contains("/v1/audio/speech"), "{text}");
    assert!(!text.contains(SPOKEN), "{text}");
}

#[tokio::test]
async fn a_success_body_over_the_cap_is_refused() {
    let listener = Listener::start(Reply::audio(vec![0; 3 * 1024 * 1024]));
    let client = client_with(|limits| limits.max_audio_bytes = 2 * 1024 * 1024);

    let error = client
        .synthesize(&listener.config(), SPOKEN)
        .await
        .unwrap_err();

    assert!(
        error.to_string().contains("more than the 2 MB limit"),
        "{error:#}"
    );
}

#[tokio::test]
async fn a_success_body_exactly_at_the_cap_is_returned() {
    let listener = Listener::start(Reply::audio(vec![0; 4096]));
    let client = client_with(|limits| limits.max_audio_bytes = 4096);

    let speech = client.synthesize(&listener.config(), SPOKEN).await.unwrap();

    assert_eq!(speech.bytes.len(), 4096);
}

#[tokio::test]
async fn a_redirect_is_an_error_and_its_target_receives_nothing() {
    let target = Listener::start(Reply::audio(vec![1]));
    let listener = Listener::start(Reply::redirect(target.url("")));

    let error = client()
        .synthesize(&listener.config(), SPOKEN)
        .await
        .unwrap_err();

    assert!(error.to_string().contains("redirect (302"), "{error:#}");
    assert_eq!(listener.received().len(), 1);
    assert!(target.received().is_empty());
}

#[tokio::test]
async fn a_server_that_does_not_answer_in_time_names_the_total_timeout() {
    let listener = Listener::start_holding();
    let client = client_with(|limits| limits.total_timeout = Duration::from_secs(1));

    let error = client
        .synthesize(&listener.config(), SPOKEN)
        .await
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("did not finish answering within 1 s"),
        "{error:#}"
    );
}

#[rstest]
#[case::timed_out_while_connecting(true, true, RequestFailure::ConnectTimeout)]
#[case::timed_out_while_answering(true, false, RequestFailure::AnswerTimeout)]
#[case::could_not_connect(false, true, RequestFailure::Connect)]
#[case::anything_else(false, false, RequestFailure::Other)]
fn a_request_failure_is_told_apart_by_its_timeout_and_connect_flags(
    #[case] timed_out: bool,
    #[case] connecting: bool,
    #[case] failure: RequestFailure,
) {
    assert_eq!(RequestFailure::of(timed_out, connecting), failure);
}

#[rstest]
#[case::connect_timeout(
    RequestFailure::ConnectTimeout,
    "Could not connect to the speech server at my-host:10157 within 7 s"
)]
#[case::answer_timeout(
    RequestFailure::AnswerTimeout,
    "The speech server at my-host:10157 did not finish answering within 90 s"
)]
#[case::connect(
    RequestFailure::Connect,
    "Could not connect to the speech server at my-host:10157"
)]
#[case::other(
    RequestFailure::Other,
    "The request to the speech server at my-host:10157 failed"
)]
fn a_failure_line_names_the_server_and_the_timeout_that_fired(
    #[case] failure: RequestFailure,
    #[case] line: &str,
) {
    let server = Server {
        host: "my-host".to_owned(),
        port: 10157,
    };
    let limits = Limits {
        connect_timeout: Duration::from_secs(7),
        total_timeout: Duration::from_secs(90),
        ..Limits::default()
    };

    assert_eq!(failure_line(failure, &server, &limits), line);
}

#[tokio::test]
async fn an_answer_cut_short_is_an_error_naming_the_server() {
    let config = SpeakConfig {
        url: cut_short_server(1000, b"RIFF"),
        ..SpeakConfig::default()
    };

    let error = client().synthesize(&config, SPOKEN).await.unwrap_err();

    assert!(
        error
            .to_string()
            .starts_with("The request to the speech server at 127.0.0.1:"),
        "{error:#}"
    );
}

#[tokio::test]
async fn a_long_content_type_with_odd_bytes_comes_back_on_one_cut_line() {
    let content_type: &'static str =
        Box::leak(format!("audio/wav; note=\t{}", "n".repeat(300)).into_boxed_str());
    let listener = Listener::start(Reply::new(200, content_type, vec![1]));

    let speech = client()
        .synthesize(&listener.config(), SPOKEN)
        .await
        .unwrap();

    let shown = speech.content_type.unwrap();
    assert_eq!(shown.chars().count(), 141);
    assert!(shown.starts_with("audio/wav; note= n"), "{shown}");
}

#[tokio::test]
async fn an_answer_as_long_as_it_promised_is_returned() {
    let config = SpeakConfig {
        url: cut_short_server(4, b"RIFF"),
        ..SpeakConfig::default()
    };

    let speech = client().synthesize(&config, SPOKEN).await.unwrap();

    assert_eq!(speech.bytes, b"RIFF");
}

#[tokio::test]
async fn a_config_that_does_not_validate_sends_nothing() {
    let listener = Listener::start(Reply::audio(vec![1]));
    let mut config = listener.config();
    config.request_method = "DELETE".to_owned();

    let error = client().synthesize(&config, SPOKEN).await.unwrap_err();

    assert!(error.to_string().contains("POST or PUT"), "{error:#}");
    assert!(listener.received().is_empty());
}

#[rstest]
#[case::text_detail(r#"{"detail": "Voice not found"}"#, "Voice not found")]
#[case::list_detail(
    r#"{"detail": [{"loc": ["body", 0, "voice"], "msg": "Field required", "input": "secret"},
                   {"msg": "Second problem"}, {"loc": ["query"]}, {"other": 1}]}"#,
    "body.0.voice: Field required; Second problem; query"
)]
#[case::detail_of_another_shape(r#"{"detail": {"input": "secret"}}"#, "")]
#[case::json_without_detail(r#"{"error": "secret"}"#, "")]
#[case::cut_json(r#"{"detail": [{"input": "secret"#, "")]
#[case::cut_json_list(r#"  [{"input": "secret"#, "")]
#[case::plain_text("  Bad Gateway \n", "Bad Gateway")]
#[case::control_characters("line one\nline\ttwo", "line one line two")]
#[case::empty("", "")]
fn server_detail_reads_only_what_is_safe_to_show(#[case] body: &str, #[case] detail: &str) {
    assert_eq!(server_detail(body.as_bytes()), detail);
}

#[test]
fn server_detail_cuts_a_long_text_detail_after_140_characters() {
    let body = json!({"detail": "d".repeat(500)}).to_string();

    let detail = server_detail(body.as_bytes());

    assert_eq!(detail.chars().count(), 141);
    assert!(detail.ends_with('…'));
}
