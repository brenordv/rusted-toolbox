use super::*;
use crate::models::ConfigSource;
use shared_eventhub_core::connection::DEFAULT_CONSUMER_GROUP;
use shared_eventhub_core::producer::{OutgoingEvent, Pacing, Routing, SendPlan};
use shared_eventhub_persistence::config::{
    AuthMethodConfig, CredentialKindConfig, SensitiveValues, TargetConfig, ToolConfig,
};
use std::io::Cursor;
use std::time::Duration;

const HOSTILE: &str = "orders\r\u{1b}]52;c;cHduZWQ=\u{7}";

fn args(body: Option<&str>, body_file: Option<PathBuf>) -> SendArgs {
    SendArgs {
        target: TargetArgs::default(),
        data: DataArgs::default(),
        body: body.map(|body| BodyText(body.to_owned())),
        body_file,
        count: 1,
        partition_key: None,
        content_type: None,
        interval_ms: None,
        yes: false,
    }
}

fn job(routing: Routing, pacing: Pacing, body_source: BodySource) -> SendJob {
    let mut config = ToolConfig::new(TargetConfig {
        namespace: Some(HOSTILE.to_owned()),
        event_hub: Some("orders".to_owned()),
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        auth: AuthMethodConfig::Credential {
            credential: CredentialKindConfig::DeveloperTools,
        },
    });
    config.storage.base_data_folder = HOSTILE.to_owned();
    SendJob {
        config,
        secrets: SensitiveValues::default(),
        save_config: false,
        working_dir: PathBuf::from("."),
        config_source: ConfigSource::Loaded,
        plan: SendPlan::new(3, routing, pacing).unwrap(),
        event: OutgoingEvent {
            body: b"secret-body".to_vec(),
            content_type: Some("application/json".to_owned()),
        },
        body_source,
    }
}

#[test]
fn an_inline_body_is_taken_as_utf8_bytes() {
    let (body, source) = load_body(&args(Some("héllo"), None)).unwrap();

    assert_eq!(body, "héllo".as_bytes());
    assert_eq!(source, BodySource::Inline);
}

#[test]
fn a_body_file_is_read_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("event.bin");
    std::fs::write(&path, [0xff, 0x00, b'x']).unwrap();

    let (body, source) = load_body(&args(None, Some(path.clone()))).unwrap();

    assert_eq!(body, [0xff, 0x00, b'x']);
    assert_eq!(source, BodySource::File(path));
}

#[test]
fn an_empty_body_file_is_allowed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.json");
    std::fs::write(&path, []).unwrap();

    let (body, _) = load_body(&args(None, Some(path))).unwrap();

    assert!(body.is_empty());
}

#[test]
fn a_missing_body_file_names_its_path() {
    let error = load_body(&args(None, Some(PathBuf::from("missing.json")))).unwrap_err();

    assert!(
        format!("{error:#}").contains("Failed to read the body file 'missing.json'"),
        "{error:#}"
    );
}

#[test]
fn a_body_file_over_the_cap_is_refused_before_it_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.bin");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(MAX_BODY_BYTES + 1)
        .unwrap();

    let error = load_body(&args(None, Some(path))).unwrap_err();

    assert!(
        error.to_string().contains("over the 20971520-byte limit"),
        "{error:#}"
    );
}

#[test]
fn the_bounded_read_refuses_a_source_one_byte_over_the_cap() {
    let error = read_capped(Cursor::new(vec![b'x'; 11]), 10, "stdin").unwrap_err();

    assert_eq!(
        error.to_string(),
        "Refusing stdin: it is over the 10-byte limit"
    );
}

#[test]
fn no_body_source_is_refused() {
    let error = load_body(&args(None, None)).unwrap_err();

    assert!(
        error.to_string().contains("--body or --body-file"),
        "{error:#}"
    );
}

#[test]
fn debug_of_the_send_arguments_shows_the_body_length_only() {
    let rendered = format!("{:?}", args(Some("card-4111"), None));

    assert!(!rendered.contains("card-4111"), "{rendered}");
    assert!(rendered.contains("BodyText(9 bytes)"), "{rendered}");
}

#[test]
fn the_bounded_read_accepts_a_source_at_the_cap() {
    let body = read_capped(Cursor::new(vec![b'x'; 10]), 10, "stdin").unwrap();

    assert_eq!(body.len(), 10);
}

#[test]
fn the_header_names_the_count_routing_pacing_and_body_size_but_not_the_body() {
    let job = job(
        Routing::partition_key("customer-17").unwrap(),
        Pacing::interval(Duration::from_millis(250)).unwrap(),
        BodySource::Stdin,
    );

    let header = send_header(&job).join("\n");

    assert!(header.contains("  - Command: send"), "{header}");
    assert!(header.contains("  - Count: 3"), "{header}");
    assert!(
        header.contains("  - Routing: partition key customer-17"),
        "{header}"
    );
    assert!(
        header.contains("  - Pacing: one event every 250 ms"),
        "{header}"
    );
    assert!(header.contains("  - Body: stdin, 11 bytes"), "{header}");
    assert!(
        header.contains("  - Content type: application/json"),
        "{header}"
    );
    assert!(!header.contains("secret-body"), "{header}");
}

#[test]
fn the_header_says_batched_and_automatic_by_default() {
    let job = job(Routing::automatic(), Pacing::batched(), BodySource::Inline);

    let header = send_header(&job).join("\n");

    assert!(header.contains("  - Routing: automatic"), "{header}");
    assert!(header.contains("  - Pacing: batched"), "{header}");
    assert!(header.contains("  - Body: --body, 11 bytes"), "{header}");
}

#[test]
fn hostile_config_values_are_escaped_in_the_send_header() {
    let job = job(Routing::automatic(), Pacing::batched(), BodySource::Inline);

    let header = send_header(&job).join("\n");

    assert!(!header.contains('\u{1b}'), "{header:?}");
    assert!(!header.contains('\r'), "{header:?}");
    assert!(header.contains("\\u{1b}"), "{header:?}");
}

#[test]
fn debug_of_a_send_job_shows_neither_the_body_nor_a_secret() {
    let mut job = job(Routing::automatic(), Pacing::batched(), BodySource::Inline);
    job.secrets.connection_string = Some(
        "Endpoint=sb://x/;SharedAccessKey=c2VjcmV0"
            .to_owned()
            .into(),
    );

    let rendered = format!("{job:?}");

    assert!(!rendered.contains("secret-body"), "{rendered}");
    assert!(!rendered.contains("SharedAccessKey"), "{rendered}");
}
