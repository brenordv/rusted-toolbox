use super::*;
use crate::cli_utils::{DataArgs, DiskModeArg, FormatArg};
use chrono::{DateTime, TimeZone, Utc};
use common_utils::string_utils::SecretString;
use rstest::rstest;
use shared_eventhub_core::filters::TimeWindow;
use shared_eventhub_persistence::export::{DiskWriteMode, ExportFormat};

const CONNECTION_STRING: &str = "Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKeyName=listen;SharedAccessKey=c2VjcmV0";
const ENV_CONNECTION_STRING: &str =
    "Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKeyName=env;SharedAccessKey=ZW52";

fn secret(value: &str) -> Option<SecretString> {
    Some(value.to_owned().into())
}

fn job(
    args: ReadArgs,
    recalled: Option<&ToolConfig>,
    environment: &Environment,
) -> Result<ReadJob> {
    read_job(
        args,
        recalled,
        environment,
        false,
        PathBuf::from("."),
        ConfigSource::Missing,
    )
}

fn error_of(args: ReadArgs, recalled: Option<&ToolConfig>, environment: &Environment) -> String {
    format!("{:#}", job(args, recalled, environment).unwrap_err())
}

fn with_target(target: TargetArgs) -> ReadArgs {
    ReadArgs {
        target,
        ..ReadArgs::default()
    }
}

fn developer() -> TargetArgs {
    TargetArgs {
        auth: Some(AuthArg::Developer),
        namespace: Some("orders.servicebus.windows.net".to_owned()),
        event_hub: Some("orders".to_owned()),
        ..TargetArgs::default()
    }
}

fn recalled_developer_config() -> ToolConfig {
    let mut config = ToolConfig::new(TargetConfig {
        namespace: Some("recalled.servicebus.windows.net".to_owned()),
        event_hub: Some("recalled-hub".to_owned()),
        consumer_group: "recalled-group".to_owned(),
        auth: AuthMethodConfig::Credential {
            credential: CredentialKindConfig::DeveloperTools,
        },
    });
    config.capture.feedback_interval_secs = 5;
    config.capture.ignore_checkpoint = true;
    config.capture.partitions = Some(vec!["3".to_owned()]);
    config.capture.filters.text = Some(TextFilter::Contains(vec!["recalled".to_owned()]));
    config.storage.base_data_folder = "recalled-data".to_owned();
    config.storage.disk_write.mode = DiskWriteMode::AtEnd;
    config.storage.disk_write.received_msg_path = "recalled-inbound".to_owned();
    config.storage.export.format = ExportFormat::Csv;
    config.storage.export.include_metadata = false;
    config.storage.export.export_folder = "recalled-exports".to_owned();
    config
}

fn at(hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 7, 2, hour, 0, 0).unwrap()
}

#[test]
fn a_connection_string_flag_selects_connection_string_auth_and_is_the_given_secret() {
    let args = with_target(TargetArgs {
        connection_string: secret(CONNECTION_STRING),
        ..TargetArgs::default()
    });

    let job = job(args, None, &Environment::default()).unwrap();

    assert!(matches!(
        job.config.target.auth,
        AuthMethodConfig::ConnectionString { .. }
    ));
    assert_eq!(
        job.secrets.connection_string.unwrap().expose(),
        CONNECTION_STRING
    );
    assert_eq!(job.config.target.consumer_group, DEFAULT_CONSUMER_GROUP);
}

#[test]
fn the_flag_beats_the_connection_string_environment_variable() {
    let environment = Environment {
        connection_string: secret(ENV_CONNECTION_STRING),
        client_secret: None,
    };
    let args = with_target(TargetArgs {
        connection_string: secret(CONNECTION_STRING),
        ..TargetArgs::default()
    });

    let job = job(args, None, &environment).unwrap();

    assert_eq!(
        job.secrets.connection_string.unwrap().expose(),
        CONNECTION_STRING
    );
}

#[test]
fn the_connection_string_environment_variable_alone_selects_connection_string_auth() {
    let environment = Environment {
        connection_string: secret(ENV_CONNECTION_STRING),
        client_secret: None,
    };
    let args = with_target(TargetArgs {
        event_hub: Some("orders".to_owned()),
        ..TargetArgs::default()
    });

    let job = job(args, None, &environment).unwrap();

    assert!(matches!(
        job.config.target.auth,
        AuthMethodConfig::ConnectionString { .. }
    ));
    assert_eq!(
        job.secrets.connection_string.unwrap().expose(),
        ENV_CONNECTION_STRING
    );
    assert_eq!(job.config.target.event_hub.as_deref(), Some("orders"));
}

#[test]
fn the_connection_string_environment_variable_stands_in_for_a_recalled_connection_string() {
    let environment = Environment {
        connection_string: secret(ENV_CONNECTION_STRING),
        client_secret: None,
    };
    let recalled = ToolConfig::new(TargetConfig {
        namespace: None,
        event_hub: Some("orders".to_owned()),
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        auth: AuthMethodConfig::ConnectionString {
            stored: StoredSecret::LocalDb,
        },
    });

    let job = job(ReadArgs::default(), Some(&recalled), &environment).unwrap();

    assert_eq!(
        job.secrets.connection_string.unwrap().expose(),
        ENV_CONNECTION_STRING
    );
}

#[test]
fn the_connection_string_environment_variable_is_ignored_under_credential_auth() {
    let environment = Environment {
        connection_string: secret(ENV_CONNECTION_STRING),
        client_secret: secret("unused-client-secret"),
    };

    let job = job(with_target(developer()), None, &environment).unwrap();

    assert!(job.secrets.connection_string.is_none());
    assert!(job.secrets.client_secret.is_none());
}

#[test]
fn the_client_secret_environment_variable_is_given_under_client_secret_auth() {
    let environment = Environment {
        connection_string: None,
        client_secret: secret("client-secret-value"),
    };
    let args = with_target(TargetArgs {
        auth: Some(AuthArg::ClientSecret),
        tenant_id: Some("tenant".to_owned()),
        client_id: Some("client".to_owned()),
        ..developer()
    });

    let job = job(args, None, &environment).unwrap();

    assert_eq!(
        job.secrets.client_secret.unwrap().expose(),
        "client-secret-value"
    );
    assert!(matches!(
        job.config.target.auth,
        AuthMethodConfig::Credential {
            credential: CredentialKindConfig::ClientSecret { .. }
        }
    ));
}

#[test]
fn key_vault_flags_with_a_vault_credential_build_a_key_vault_target() {
    let args = with_target(TargetArgs {
        auth: Some(AuthArg::ManagedIdentity),
        keyvault_url: Some("https://my-vault.vault.azure.net".to_owned()),
        keyvault_secret: Some("eh-orders".to_owned()),
        ..TargetArgs::default()
    });

    let job = job(args, None, &Environment::default()).unwrap();

    assert_eq!(
        job.config.target.auth,
        AuthMethodConfig::KeyVaultConnectionString {
            vault_url: "https://my-vault.vault.azure.net".to_owned(),
            secret_name: "eh-orders".to_owned(),
            credential: CredentialKindConfig::ManagedIdentity,
        }
    );
}

#[test]
fn a_recalled_config_supplies_every_setting_no_flag_gives() {
    let recalled = recalled_developer_config();

    let job = job(
        ReadArgs::default(),
        Some(&recalled),
        &Environment::default(),
    )
    .unwrap();

    assert_eq!(job.config.target, recalled.target);
    assert_eq!(job.config.capture, recalled.capture);
    assert_eq!(job.config.storage, recalled.storage);
}

#[test]
fn flags_beat_the_recalled_config_field_by_field() {
    let recalled = recalled_developer_config();
    let args = ReadArgs {
        target: TargetArgs {
            event_hub: Some("flag-hub".to_owned()),
            consumer_group: Some("flag-group".to_owned()),
            ..TargetArgs::default()
        },
        data: DataArgs {
            base_data_folder: Some("flag-data".to_owned()),
            no_config: false,
        },
        partitions: vec!["0".to_owned(), "1".to_owned()],
        feedback_interval: Some(2),
        text: TextFilterArgs {
            dump_filter: Some(vec!["flag".to_owned()]),
            filter_regex: None,
        },
        time: TimeFilterArgs {
            newer_than: Some(at(12)),
            ..TimeFilterArgs::default()
        },
        disk_mode: Some(DiskModeArg::Immediate),
        received_msg_path: Some("flag-inbound".to_owned()),
        ..ReadArgs::default()
    };

    let job = job(args, Some(&recalled), &Environment::default()).unwrap();

    let config = &job.config;
    assert_eq!(
        config.target.namespace.as_deref(),
        Some("recalled.servicebus.windows.net")
    );
    assert_eq!(config.target.event_hub.as_deref(), Some("flag-hub"));
    assert_eq!(config.target.consumer_group, "flag-group");
    assert_eq!(
        config.capture.partitions,
        Some(vec!["0".to_owned(), "1".to_owned()])
    );
    assert_eq!(config.capture.feedback_interval_secs, 2);
    assert_eq!(
        config.capture.filters.text,
        Some(TextFilter::Contains(vec!["flag".to_owned()]))
    );
    assert_eq!(
        config.capture.filters.time,
        Some(TimeFilter::NewerThan(at(12)))
    );
    assert_eq!(config.storage.base_data_folder, "flag-data");
    assert_eq!(config.storage.disk_write.mode, DiskWriteMode::Immediate);
    assert_eq!(config.storage.disk_write.received_msg_path, "flag-inbound");
    assert_eq!(config.storage.export, recalled.storage.export);
}

#[test]
fn an_auth_flag_replaces_the_recalled_auth_namespace_and_hub_together() {
    let recalled = recalled_developer_config();
    let args = with_target(TargetArgs {
        connection_string: secret(CONNECTION_STRING),
        ..TargetArgs::default()
    });

    let job = job(args, Some(&recalled), &Environment::default()).unwrap();

    assert!(matches!(
        job.config.target.auth,
        AuthMethodConfig::ConnectionString { .. }
    ));
    assert_eq!(job.config.target.namespace, None);
    assert_eq!(job.config.target.event_hub, None);
    assert_eq!(job.config.target.consumer_group, "recalled-group");
}

#[test]
fn defaults_apply_with_neither_flags_nor_config() {
    let job = job(with_target(developer()), None, &Environment::default()).unwrap();

    let config = &job.config;
    assert_eq!(config.capture.partitions, None);
    assert_eq!(config.capture.feedback_interval_secs, 1);
    assert!(!config.capture.ignore_checkpoint);
    assert_eq!(config.storage.base_data_folder, ".eh-data");
    assert_eq!(config.storage.disk_write.mode, DiskWriteMode::Off);
    assert_eq!(config.storage.disk_write.received_msg_path, "inbound");
}

#[test]
fn nothing_to_read_fails_with_guidance() {
    let message = error_of(ReadArgs::default(), None, &Environment::default());

    assert!(message.contains("No event hub to read"), "{message}");
}

#[rstest]
#[case::empty("")]
#[case::spaces("   ")]
fn an_empty_consumer_group_is_refused(#[case] group: &str) {
    let args = with_target(TargetArgs {
        consumer_group: Some(group.to_owned()),
        ..developer()
    });

    let message = error_of(args, None, &Environment::default());

    assert!(message.contains("consumer group is empty"), "{message}");
}

#[rstest]
#[case::credential_without_namespace(
    TargetArgs { namespace: None, ..developer() },
    "needs --namespace"
)]
#[case::credential_without_hub(
    TargetArgs { event_hub: None, ..developer() },
    "needs -e/--entity-path"
)]
#[case::connection_string_with_other_auth(
    TargetArgs { connection_string: secret(CONNECTION_STRING), ..developer() },
    "cannot be combined with --auth developer"
)]
#[case::connection_string_with_key_vault(
    TargetArgs {
        connection_string: secret(CONNECTION_STRING),
        keyvault_url: Some("https://v.vault.azure.net".to_owned()),
        keyvault_secret: Some("s".to_owned()),
        ..TargetArgs::default()
    },
    "--keyvault-url fetches the connection string"
)]
#[case::connection_string_with_client_ids(
    TargetArgs {
        connection_string: secret(CONNECTION_STRING),
        tenant_id: Some("t".to_owned()),
        ..TargetArgs::default()
    },
    "apply to --auth client-secret only"
)]
#[case::client_ids_without_auth(
    TargetArgs { client_id: Some("c".to_owned()), ..TargetArgs::default() },
    "need --auth client-secret"
)]
#[case::client_ids_with_developer(
    TargetArgs { tenant_id: Some("t".to_owned()), ..developer() },
    "apply to --auth client-secret only"
)]
#[case::client_secret_without_ids(
    TargetArgs { auth: Some(AuthArg::ClientSecret), ..developer() },
    "needs --tenant-id and --client-id"
)]
#[case::client_secret_with_blank_client(
    TargetArgs {
        auth: Some(AuthArg::ClientSecret),
        tenant_id: Some("t".to_owned()),
        client_id: Some(" ".to_owned()),
        ..developer()
    },
    "needs --tenant-id and --client-id"
)]
#[case::key_vault_url_alone(
    TargetArgs {
        auth: Some(AuthArg::Developer),
        keyvault_url: Some("https://v.vault.azure.net".to_owned()),
        ..TargetArgs::default()
    },
    "go together"
)]
#[case::key_vault_without_auth(
    TargetArgs {
        keyvault_url: Some("https://v.vault.azure.net".to_owned()),
        keyvault_secret: Some("s".to_owned()),
        ..TargetArgs::default()
    },
    "needs --auth naming the credential for the vault"
)]
#[case::key_vault_with_connection_string_auth(
    TargetArgs {
        auth: Some(AuthArg::ConnectionString),
        keyvault_url: Some("https://v.vault.azure.net".to_owned()),
        keyvault_secret: Some("s".to_owned()),
        ..TargetArgs::default()
    },
    "--keyvault-url fetches the connection string"
)]
#[case::key_vault_developer_with_client_ids(
    TargetArgs {
        auth: Some(AuthArg::Developer),
        keyvault_url: Some("https://v.vault.azure.net".to_owned()),
        keyvault_secret: Some("s".to_owned()),
        client_id: Some("c".to_owned()),
        ..TargetArgs::default()
    },
    "apply to --auth client-secret only"
)]
fn inconsistent_target_flags_are_refused_naming_the_flag(
    #[case] target: TargetArgs,
    #[case] expected: &str,
) {
    let message = error_of(with_target(target), None, &Environment::default());

    assert!(message.contains(expected), "{message}");
}

#[test]
fn debug_of_the_job_never_shows_a_secret() {
    let environment = Environment {
        connection_string: None,
        client_secret: secret("client-secret-value"),
    };
    let args = with_target(TargetArgs {
        connection_string: secret(CONNECTION_STRING),
        ..TargetArgs::default()
    });
    let rendered_args = format!("{args:?}");

    let job = job(args, None, &environment).unwrap();

    let rendered_job = format!("{job:?}");
    for rendered in [rendered_args, rendered_job] {
        assert!(!rendered.contains("SharedAccessKey"), "{rendered}");
        assert!(!rendered.contains("client-secret-value"), "{rendered}");
    }
}

#[test]
fn export_takes_recalled_export_settings_and_its_own_filters_only() {
    let recalled = recalled_developer_config();

    let job = export_job(ExportArgs::default(), Some(&recalled), ConfigSource::Loaded);

    assert_eq!(job.export, recalled.storage.export);
    assert_eq!(job.base_data_folder, PathBuf::from("recalled-data"));
    assert_eq!(job.filters, CaptureFilters::default());
}

#[test]
fn export_flags_beat_the_recalled_export_settings() {
    let recalled = recalled_developer_config();
    let args = ExportArgs {
        data: DataArgs {
            base_data_folder: Some("flag-data".to_owned()),
            no_config: false,
        },
        format: Some(FormatArg::Json),
        export_folder: Some("flag-exports".to_owned()),
        condense_output: true,
        include_metadata: Some(true),
        use_local_time: true,
        ignore_export_log: true,
        text: TextFilterArgs {
            dump_filter: None,
            filter_regex: Some(TextFilter::regex("order-\\d+").unwrap()),
        },
        time: TimeFilterArgs {
            between: Some(TimeWindow {
                start: at(1),
                end: at(2),
            }),
            ..TimeFilterArgs::default()
        },
    };

    let job = export_job(args, Some(&recalled), ConfigSource::Loaded);

    assert_eq!(job.base_data_folder, PathBuf::from("flag-data"));
    assert_eq!(
        job.export,
        ExportSettings {
            format: ExportFormat::Json,
            include_metadata: true,
            condense_output: true,
            use_local_time: true,
            export_folder: "flag-exports".to_owned(),
            skip_already_exported: false,
        }
    );
    assert_eq!(
        job.filters.time,
        Some(TimeFilter::Between {
            start: at(1),
            end: at(2)
        })
    );
    assert_eq!(
        job.filters.text,
        Some(TextFilter::regex("order-\\d+").unwrap())
    );
}

#[test]
fn export_defaults_apply_without_config() {
    let job = export_job(ExportArgs::default(), None, ConfigSource::Missing);

    assert_eq!(job.export, ExportSettings::default());
    assert_eq!(job.base_data_folder, PathBuf::from(".eh-data"));
}

#[test]
fn parse_regex_rejects_an_invalid_pattern() {
    let error = parse_regex("(unclosed").unwrap_err();

    assert!(error.contains("Invalid text filter regex"), "{error}");
}
