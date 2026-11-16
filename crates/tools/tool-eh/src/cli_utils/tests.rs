use super::*;
use crate::models::ConfigSource;
use clap::CommandFactory;
use clap::error::ErrorKind;
use shared_eventhub_core::connection::DEFAULT_CONSUMER_GROUP;
use shared_eventhub_persistence::config::{
    AuthMethodConfig, CredentialKindConfig, SensitiveValues, StoredSecret, TargetConfig,
};
use std::path::PathBuf;

fn parse(argv: &[&str]) -> CliArgs {
    CliArgs::try_parse_from(argv).unwrap()
}

fn parse_error(argv: &[&str]) -> ErrorKind {
    CliArgs::try_parse_from(argv).unwrap_err().kind()
}

fn read_args(argv: &[&str]) -> ReadArgs {
    match parse(argv).command {
        Commands::Read(args) => *args,
        Commands::Export(_) | Commands::Send(_) => panic!("expected the read subcommand"),
    }
}

fn export_args(argv: &[&str]) -> ExportArgs {
    match parse(argv).command {
        Commands::Export(args) => *args,
        Commands::Read(_) | Commands::Send(_) => panic!("expected the export subcommand"),
    }
}

fn send_args(argv: &[&str]) -> SendArgs {
    match parse(argv).command {
        Commands::Send(args) => *args,
        Commands::Read(_) | Commands::Export(_) => panic!("expected the send subcommand"),
    }
}

const HOSTILE: &str = "orders\r\u{1b}]52;c;cHduZWQ=\u{7}";

fn hostile_config() -> ToolConfig {
    let mut config = ToolConfig::new(TargetConfig {
        namespace: Some(HOSTILE.to_owned()),
        event_hub: Some("orders".to_owned()),
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        auth: AuthMethodConfig::Credential {
            credential: CredentialKindConfig::DeveloperTools,
        },
    });
    config.storage.base_data_folder = HOSTILE.to_owned();
    config
}

fn assert_escaped(rendered: &str) {
    assert!(!rendered.contains('\u{1b}'), "{rendered:?}");
    assert!(!rendered.contains('\r'), "{rendered:?}");
    assert!(!rendered.contains('\u{7}'), "{rendered:?}");
    assert!(rendered.contains("\\u{1b}"), "{rendered:?}");
}

#[test]
fn cli_definition_has_no_conflicting_flags() {
    CliArgs::command().debug_assert();
}

#[test]
fn bare_invocation_shows_help() {
    assert_eq!(
        parse_error(&["eh"]),
        ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    );
}

#[test]
fn shared_flags_go_before_the_subcommand() {
    let args = parse(&["eh", "--verbose", "--app-header", "read"]);

    assert!(args.common.verbose);
    assert!(args.common.app_header);
}

#[test]
fn shared_flags_after_the_subcommand_are_a_usage_error() {
    assert_eq!(
        parse_error(&["eh", "read", "--verbose"]),
        ErrorKind::UnknownArgument
    );
}

#[test]
fn read_flags_land_in_their_fields() {
    let args = read_args(&[
        "eh",
        "read",
        "-s",
        "Endpoint=sb://x/;SharedAccessKey=k",
        "-e",
        "orders",
        "-g",
        "group",
        "-b",
        "data",
        "-p",
        "0",
        "-p",
        "2",
        "-i",
        "--feedback-interval",
        "3",
        "--dump-filter",
        "a,b",
        "--newer-than",
        "2026-07-02",
        "--disk-mode",
        "at-end",
        "-r",
        "dumps",
        "--dump-content-only",
        "--save-config",
    ]);

    assert_eq!(
        args.target.connection_string.unwrap().expose(),
        "Endpoint=sb://x/;SharedAccessKey=k"
    );
    assert_eq!(args.target.event_hub.as_deref(), Some("orders"));
    assert_eq!(args.consumer_group.as_deref(), Some("group"));
    assert!(args.target.save_config);
    assert_eq!(args.data.base_data_folder.as_deref(), Some("data"));
    assert_eq!(args.partitions, ["0", "2"]);
    assert!(args.ignore_checkpoint);
    assert_eq!(args.feedback_interval, Some(3));
    assert_eq!(
        args.text.dump_filter,
        Some(vec!["a".to_owned(), "b".to_owned()])
    );
    assert!(args.time.newer_than.is_some());
    assert_eq!(args.disk_mode, Some(DiskModeArg::AtEnd));
    assert_eq!(args.received_msg_path.as_deref(), Some("dumps"));
    assert!(args.dump_content_only);
}

#[test]
fn flags_config_json_can_supply_stay_unset_when_not_given() {
    let args = read_args(&["eh", "read"]);

    assert!(args.consumer_group.is_none());
    assert!(args.data.base_data_folder.is_none());
    assert!(args.feedback_interval.is_none());
    assert!(args.disk_mode.is_none());
    assert!(args.received_msg_path.is_none());
}

#[test]
fn export_flags_land_in_their_fields() {
    let args = export_args(&[
        "eh",
        "export",
        "-f",
        "csv",
        "--include-metadata",
        "false",
        "--export-folder",
        "out",
        "--condense-output",
        "--use-local-time",
        "-i",
        "--filter-regex",
        "order-\\d+",
        "--between",
        "2026-07-02..2026-07-03",
    ]);

    assert_eq!(args.format, Some(FormatArg::Csv));
    assert_eq!(args.include_metadata, Some(false));
    assert_eq!(args.export_folder.as_deref(), Some("out"));
    assert!(args.condense_output);
    assert!(args.use_local_time);
    assert!(args.ignore_export_log);
    assert!(args.text.filter_regex.is_some());
    assert!(args.time.between.is_some());
}

#[rstest::rstest]
#[case::two_time_filters(&["eh", "read", "--newer-than", "2026-07-02", "--older-than", "2026-07-03"])]
#[case::both_text_filters(&["eh", "export", "--dump-filter", "a", "--filter-regex", "b"])]
fn conflicting_filters_are_a_usage_error(#[case] argv: &[&str]) {
    assert_eq!(parse_error(argv), ErrorKind::ArgumentConflict);
}

#[rstest::rstest]
#[case::trailing_comma(&["--dump-filter", "a,"], &["a"])]
#[case::repeated_flag(&["--dump-filter", "a", "--dump-filter", "b"], &["a", "b"])]
fn dump_filter_values_from_the_command_line_end_as_keywords(
    #[case] filter_flags: &[&str],
    #[case] expected: &[&str],
) {
    let mut argv = vec![
        "eh",
        "read",
        "--auth",
        "developer",
        "--namespace",
        "orders.servicebus.windows.net",
        "-e",
        "orders",
    ];
    argv.extend_from_slice(filter_flags);

    let job = read_job(
        read_args(&argv),
        None,
        &Environment::default(),
        false,
        PathBuf::from("."),
        ConfigSource::Missing,
    )
    .unwrap();

    assert_eq!(
        job.config.capture.filters.text,
        Some(TextFilter::Contains(
            expected.iter().map(|value| (*value).to_owned()).collect()
        ))
    );
}

#[test]
fn include_metadata_takes_only_true_or_false() {
    assert_eq!(
        parse_error(&["eh", "export", "--include-metadata", "yes"]),
        ErrorKind::InvalidValue
    );
}

#[rstest::rstest]
#[case::zero_feedback_interval(&["eh", "read", "--feedback-interval", "0"])]
#[case::bad_timestamp(&["eh", "read", "--older-than", "noon"])]
#[case::date_for_during(&["eh", "read", "--during", "2026-07-02"])]
#[case::bad_regex(&["eh", "read", "--filter-regex", "(unclosed"])]
fn invalid_values_are_a_usage_error(#[case] argv: &[&str]) {
    assert_eq!(parse_error(argv), ErrorKind::ValueValidation);
}

#[test]
fn send_flags_land_in_their_fields() {
    let args = send_args(&[
        "eh",
        "send",
        "-e",
        "orders",
        "--body",
        "{}",
        "-n",
        "500",
        "--partition-key",
        "customer-17",
        "--content-type",
        "application/json",
        "--interval-ms",
        "250",
        "--yes",
    ]);

    assert_eq!(args.target.event_hub.as_deref(), Some("orders"));
    assert_eq!(args.body.map(|body| body.0).as_deref(), Some("{}"));
    assert_eq!(args.count, 500);
    assert_eq!(args.partition_key.as_deref(), Some("customer-17"));
    assert_eq!(args.content_type.as_deref(), Some("application/json"));
    assert_eq!(args.interval_ms, Some(250));
    assert!(args.yes);
}

#[test]
fn repeated_property_flags_arrive_raw_in_order_with_the_correlation_id() {
    let args = send_args(&[
        "eh",
        "send",
        "--body",
        "x",
        "--property",
        "c=d",
        "--property",
        "no equals sign",
        "--correlation-id",
        "trace-1",
    ]);

    let properties: Vec<&str> = args.properties.iter().map(|text| text.0.as_str()).collect();
    assert_eq!(properties, ["c=d", "no equals sign"]);
    assert_eq!(
        args.correlation_id.map(|id| id.0).as_deref(),
        Some("trace-1")
    );
}

#[test]
fn a_bad_property_fails_before_any_target_is_named() {
    let error = send_command_for(&["eh", "send", "--body", "x", "--property", "ab"]).unwrap_err();

    assert_eq!(
        format!("{error:#}"),
        "--property 1: no '=' (write it as key=value)"
    );
}

#[test]
fn send_defaults_to_one_batched_event() {
    let args = send_args(&["eh", "send", "--body-file", "-"]);

    assert_eq!(args.count, 1);
    assert_eq!(args.interval_ms, None);
    assert_eq!(args.body_file.as_deref(), Some(std::path::Path::new("-")));
}

#[rstest::rstest]
#[case::no_body(&["eh", "send"], ErrorKind::MissingRequiredArgument)]
#[case::two_bodies(&["eh", "send", "--body", "x", "--body-file", "y"], ErrorKind::ArgumentConflict)]
#[case::zero_count(&["eh", "send", "--body", "x", "-n", "0"], ErrorKind::ValueValidation)]
#[case::count_over_the_maximum(&["eh", "send", "--body", "x", "-n", "1000001"], ErrorKind::ValueValidation)]
#[case::zero_interval(&["eh", "send", "--body", "x", "--interval-ms", "0"], ErrorKind::ValueValidation)]
#[case::interval_over_the_maximum(&["eh", "send", "--body", "x", "--interval-ms", "3600001"], ErrorKind::ValueValidation)]
#[case::consumer_group(&["eh", "send", "--body", "x", "-g", "group"], ErrorKind::UnknownArgument)]
#[case::partition_id(&["eh", "send", "--body", "x", "--partition", "0"], ErrorKind::UnknownArgument)]
#[case::dry_run_without_template(&["eh", "send", "--body", "x", "--dry-run"], ErrorKind::MissingRequiredArgument)]
#[case::dry_run_with_save_config(&["eh", "send", "--body", "x", "--template", "--dry-run", "--save-config"], ErrorKind::ArgumentConflict)]
#[case::dry_run_with_partition_key(&["eh", "send", "--body", "x", "--template", "--dry-run", "--partition-key", "k"], ErrorKind::ArgumentConflict)]
#[case::dry_run_with_interval(&["eh", "send", "--body", "x", "--template", "--dry-run", "--interval-ms", "5"], ErrorKind::ArgumentConflict)]
#[case::dry_run_with_yes(&["eh", "send", "--body", "x", "--template", "--dry-run", "--yes"], ErrorKind::ArgumentConflict)]
#[case::dry_run_with_property(&["eh", "send", "--body", "x", "--template", "--dry-run", "--property", "a=b"], ErrorKind::ArgumentConflict)]
#[case::dry_run_with_correlation_id(&["eh", "send", "--body", "x", "--template", "--dry-run", "--correlation-id", "c"], ErrorKind::ArgumentConflict)]
fn bad_send_arguments_are_usage_errors(#[case] argv: &[&str], #[case] expected: ErrorKind) {
    assert_eq!(parse_error(argv), expected);
}

fn send_command_for(argv: &[&str]) -> Result<EhCommand> {
    send_command(
        send_args(argv),
        None,
        &Environment::default(),
        PathBuf::from("."),
        ConfigSource::Missing,
    )
}

#[test]
fn a_template_dry_run_resolves_to_a_dry_run_without_a_target() {
    let command = send_command_for(&[
        "eh",
        "send",
        "--template",
        "--dry-run",
        "-n",
        "3",
        "--body",
        "{{random.integer}} {{person.email}}",
    ])
    .unwrap();

    let EhCommand::DryRun(job) = command else {
        panic!("expected a dry run, got {command:?}");
    };
    assert_eq!(job.count, 3);
    assert_eq!(job.template.placeholder_count(), 2);
}

#[test]
fn a_template_send_carries_the_compiled_template_and_the_source_body() {
    let command = send_command_for(&[
        "eh",
        "send",
        "--auth",
        "developer",
        "--namespace",
        "orders.servicebus.windows.net",
        "-e",
        "orders",
        "--template",
        "--body",
        "{{random.integer}}",
    ])
    .unwrap();

    let EhCommand::Send(job) = command else {
        panic!("expected a send, got {command:?}");
    };
    assert_eq!(job.event.body, b"{{random.integer}}");
    assert_eq!(job.template.map(|t| t.placeholder_count()), Some(1));
}

#[test]
fn a_send_without_template_keeps_the_braces_literal() {
    let command = send_command_for(&[
        "eh",
        "send",
        "--auth",
        "developer",
        "--namespace",
        "orders.servicebus.windows.net",
        "-e",
        "orders",
        "--body",
        "{{person.email}}",
    ])
    .unwrap();

    let EhCommand::Send(job) = command else {
        panic!("expected a send, got {command:?}");
    };
    assert!(job.template.is_none());
    assert_eq!(job.event.body, b"{{person.email}}");
}

#[test]
fn a_template_that_does_not_compile_fails_naming_the_position_before_any_target() {
    let error = send_command_for(&["eh", "send", "--template", "--body", "{{person.fullname}}"])
        .unwrap_err();

    assert_eq!(
        format!("{error:#}"),
        "The --template body does not compile: line 1, column 1: unknown type 'person.fullname'"
    );
}

#[test]
fn a_dry_run_flag_without_a_template_is_refused() {
    let mut args = send_args(&["eh", "send", "--body", "x", "--template", "--dry-run"]);
    args.template = false;

    let error = send_command(
        args,
        None,
        &Environment::default(),
        PathBuf::from("."),
        ConfigSource::Missing,
    )
    .unwrap_err();

    assert_eq!(error.to_string(), "--dry-run needs --template");
}

#[test]
fn the_send_long_help_lists_placeholders_and_options() {
    let mut command = CliArgs::command();
    let help = command
        .find_subcommand_mut("send")
        .unwrap()
        .render_long_help()
        .to_string();

    assert!(help.contains("person: person.first-name"), "{help}");
    assert!(help.contains("commerce.product-description"), "{help}");
    assert!(help.contains("min=N max=N"), "{help}");
    assert!(help.contains("clock: now, utc-now"), "{help}");
}

#[test]
fn the_count_and_interval_maximums_are_accepted() {
    let args = send_args(&[
        "eh",
        "send",
        "--body",
        "x",
        "-n",
        "1000000",
        "--interval-ms",
        "3600000",
    ]);

    assert_eq!(args.count, 1_000_000);
    assert_eq!(args.interval_ms, Some(3_600_000));
}

#[test]
fn debug_of_the_parsed_arguments_redacts_the_connection_string() {
    let args = parse(&["eh", "read", "-s", "Endpoint=sb://x/;SharedAccessKey=k"]);

    let rendered = format!("{args:?}");

    assert!(!rendered.contains("SharedAccessKey"), "{rendered}");
}

#[test]
fn a_foreign_config_gets_a_one_line_notice() {
    assert_eq!(
        config_notice(ConfigSource::Foreign, None).as_deref(),
        Some("config.json here is not an eh config; ignored")
    );
}

#[rstest::rstest]
#[case::missing(ConfigSource::Missing)]
#[case::disabled(ConfigSource::Disabled)]
fn no_config_means_no_notice(#[case] source: ConfigSource) {
    assert_eq!(config_notice(source, None), None);
}

#[test]
fn a_recalled_config_notice_shows_target_and_folders() {
    let config = ToolConfig::new(TargetConfig {
        namespace: None,
        event_hub: Some("orders".to_owned()),
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        auth: AuthMethodConfig::ConnectionString {
            stored: StoredSecret::LocalDb,
        },
    });

    let notice = config_notice(ConfigSource::Loaded, Some(&config)).unwrap();

    assert_eq!(
        notice,
        "Using config.json: namespace (from the connection string), event hub orders, auth \
         connection string, consumer group $Default, data folder .eh-data, dump folder inbound \
         (flags override these)"
    );
}

#[test]
fn hostile_config_values_are_escaped_in_the_notice() {
    let notice = config_notice(ConfigSource::Loaded, Some(&hostile_config())).unwrap();

    assert_escaped(&notice);
}

#[test]
fn hostile_config_values_are_escaped_in_the_read_header() {
    let job = ReadJob {
        config: hostile_config(),
        secrets: SensitiveValues::default(),
        save_config: false,
        verbose: false,
        working_dir: PathBuf::from("."),
        config_source: ConfigSource::Loaded,
    };

    let header = read_header(&job).join("\n");

    assert_escaped(&header);
    assert!(
        header.contains("  - Auth: developer tools credential"),
        "{header}"
    );
}

#[test]
fn the_export_header_names_the_resolved_export_folder() {
    let job = ExportJob {
        base_data_folder: PathBuf::from("data"),
        export: Default::default(),
        filters: Default::default(),
        config_source: ConfigSource::Missing,
    };

    let header = export_header(&job).join("\n");

    let expected = PathBuf::from("data").join("exports").display().to_string();
    assert!(
        header.contains(&format!("  - Export folder: {expected}")),
        "{header}"
    );
    assert!(header.contains("  - config.json: none"), "{header}");
}
