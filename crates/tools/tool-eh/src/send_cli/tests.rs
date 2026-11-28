use super::*;
use crate::models::ConfigSource;
use eh_core::connection::DEFAULT_CONSUMER_GROUP;
use eh_core::producer::{OutgoingEvent, Pacing, Routing, SendPlan};
use eh_persistence::config::{
    AuthMethodConfig, CredentialKindConfig, SensitiveValues, TargetConfig, ToolConfig,
};
use eh_persistence::paths::DataPaths;
use eh_persistence::workspace::ConfigHome;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

const HOSTILE: &str = "orders\r\u{1b}]52;c;cHduZWQ=\u{7}";

fn args(body: Option<&str>, body_file: Option<PathBuf>) -> SendArgs {
    SendArgs {
        target: TargetArgs::default(),
        data: DataArgs::default(),
        body: body.map(|body| ArgText(body.to_owned())),
        body_file,
        count: 1,
        partition_key: None,
        content_type: None,
        properties: Vec::new(),
        correlation_id: None,
        interval_ms: None,
        yes: false,
        template: false,
        dry_run: false,
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
        home: ConfigHome::CurrentFolder(PathBuf::from(".")),
        paths: DataPaths::new(HOSTILE),
        config_source: ConfigSource::Loaded,
        plan: SendPlan::new(3, routing, pacing).unwrap(),
        event: OutgoingEvent {
            body: b"secret-body".to_vec(),
            content_type: Some("application/json".to_owned()),
            ..OutgoingEvent::default()
        },
        template: None,
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
    assert!(rendered.contains("ArgText(9 bytes)"), "{rendered}");
}

#[test]
fn debug_of_the_send_arguments_hides_properties_and_the_correlation_id() {
    let mut args = args(Some("x"), None);
    args.properties = vec![ArgText("customer=zyxwv".to_owned())];
    args.correlation_id = Some(ArgText("trace-abc".to_owned()));

    let rendered = format!("{args:?}");

    assert!(!rendered.contains("customer"), "{rendered}");
    assert!(!rendered.contains("zyxwv"), "{rendered}");
    assert!(!rendered.contains("trace-abc"), "{rendered}");
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
fn the_header_says_none_without_a_content_type() {
    let mut job = job(Routing::automatic(), Pacing::batched(), BodySource::Inline);
    job.event.content_type = None;

    let header = send_header(&job).join("\n");

    assert!(header.contains("  - Content type: none"), "{header}");
}

#[test]
fn the_header_says_none_without_a_correlation_id_or_properties() {
    let job = job(Routing::automatic(), Pacing::batched(), BodySource::Inline);

    let header = send_header(&job).join("\n");

    assert!(header.contains("  - Correlation id: none"), "{header}");
    assert!(header.contains("  - Properties: none"), "{header}");
}

#[test]
fn the_header_names_the_correlation_id_and_property_keys_but_no_value() {
    let mut job = job(Routing::automatic(), Pacing::batched(), BodySource::Inline);
    job.event.correlation_id = Some("abc".to_owned());
    job.event.properties = [("c", "secret-d"), ("a", "secret-b")]
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .into();

    let header = send_header(&job).join("\n");

    assert!(header.contains("  - Correlation id: abc"), "{header}");
    assert!(header.contains("  - Properties: 2: a, c"), "{header}");
    assert!(!header.contains("secret-"), "{header}");
}

#[test]
fn the_header_names_five_property_keys_and_counts_the_rest() {
    let mut job = job(Routing::automatic(), Pacing::batched(), BodySource::Inline);
    job.event.properties = ["a", "b", "c", "d", "e", "f", "g"]
        .map(|key| (key.to_owned(), String::new()))
        .into();

    let header = send_header(&job).join("\n");

    assert!(
        header.contains("  - Properties: 7: a, b, c, d, e and 2 more"),
        "{header}"
    );
}

#[test]
fn hostile_property_keys_and_ids_are_escaped_in_the_send_header() {
    let mut job = job(Routing::automatic(), Pacing::batched(), BodySource::Inline);
    job.config.target.namespace = Some("orders".to_owned());
    job.config.storage.base_data_folder = "data".to_owned();
    job.paths = DataPaths::new("data");
    job.event.correlation_id = Some(HOSTILE.to_owned());
    job.event.properties = [(HOSTILE.to_owned(), String::new())].into();

    let header = send_header(&job).join("\n");

    assert!(!header.contains('\u{1b}'), "{header:?}");
    assert!(!header.contains('\r'), "{header:?}");
    assert_eq!(header.matches("\\u{1b}").count(), 2, "{header:?}");
}

#[test]
fn the_header_says_no_template_by_default() {
    let job = job(Routing::automatic(), Pacing::batched(), BodySource::Inline);

    let header = send_header(&job).join("\n");

    assert!(header.contains("  - Template: no"), "{header}");
}

#[test]
fn the_header_counts_the_templates_placeholders() {
    let mut job = job(Routing::automatic(), Pacing::batched(), BodySource::Inline);
    job.template = Some(Arc::new(
        compile_template(b"{{person.email}} {{random.integer}}").unwrap(),
    ));

    let header = send_header(&job).join("\n");

    assert!(
        header.contains("  - Template: yes, 2 placeholders"),
        "{header}"
    );
}

#[test]
fn the_dry_run_header_names_the_command_count_and_template() {
    let job = DryRunJob {
        template: Arc::new(compile_template(b"{{person.email}}").unwrap()),
        count: 7,
        body_source: BodySource::Stdin,
        body_len: 16,
    };

    let header = dry_run_header(&job).join("\n");

    assert!(header.contains("  - Command: send --dry-run"), "{header}");
    assert!(header.contains("  - Count: 7"), "{header}");
    assert!(header.contains("  - Body: stdin, 16 bytes"), "{header}");
    assert!(
        header.contains("  - Template: yes, 1 placeholder"),
        "{header}"
    );
}

#[test]
fn the_dry_run_header_counts_clock_placeholders() {
    let job = DryRunJob {
        template: Arc::new(compile_template(b"{{now}} {{utc-now}}").unwrap()),
        count: 1,
        body_source: BodySource::Inline,
        body_len: 19,
    };

    let header = dry_run_header(&job).join("\n");

    assert!(
        header.contains("  - Template: yes, 2 placeholders"),
        "{header}"
    );
}

#[test]
fn a_template_of_only_now_gets_no_notice() {
    let template = compile_template(b"{\"at\": \"{{now}}\"}").unwrap();

    assert!(template_notices(&template).is_empty());
}

#[test]
fn a_template_body_that_is_not_utf8_names_the_offset() {
    let error = compile_template(b"ok \xff rest").unwrap_err();

    assert_eq!(
        error.to_string(),
        "The --template body is not UTF-8: the byte at offset 3 is not valid"
    );
}

#[test]
fn a_template_with_no_placeholders_gets_a_notice() {
    let template = compile_template(b"plain").unwrap();

    assert_eq!(
        template_notices(&template),
        ["The template has no placeholders; every event is the same."]
    );
}

#[test]
fn an_inert_locale_gets_a_notice_and_a_plain_template_none() {
    let inert = compile_template(b"{{random.integer locale=pt-br}}").unwrap();
    let plain = compile_template(b"{{person.city}}").unwrap();

    assert_eq!(template_notices(&inert).len(), 1);
    assert!(template_notices(&plain).is_empty());
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
