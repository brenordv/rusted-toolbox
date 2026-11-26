use super::*;
use crate::cli_utils::{DataArgs, DiskModeArg, FormatArg};
use crate::send_cli::ArgText;
use chrono::{DateTime, TimeZone, Utc};
use common_utils::string_utils::SecretString;
use eh_core::filters::TimeWindow;
use eh_persistence::config::resolve_target_with;
use eh_persistence::export::{DiskWriteMode, ExportFormat};
use eh_persistence::paths::DataPaths;
use eh_persistence::workspace::{ProfileName, Workspace};
use rstest::rstest;
use std::path::{Path, PathBuf};

const CONNECTION_STRING: &str = "Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKeyName=listen;SharedAccessKey=c2VjcmV0";
const ENV_CONNECTION_STRING: &str =
    "Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKeyName=env;SharedAccessKey=ZW52";
const ENV_CONNECTION_STRING_FOR_RECALLED_HUB: &str = "Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKeyName=env;SharedAccessKey=ZW52;EntityPath=recalled-hub";
const ENV_CONNECTION_STRING_FOR_ANOTHER_HUB: &str = "Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKeyName=env;SharedAccessKey=ZW52;EntityPath=another-hub";

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
        here(),
        ConfigSource::Missing,
    )
}

fn here() -> ConfigHome {
    ConfigHome::CurrentFolder(PathBuf::from("."))
}

/// A profile home named `name` under the workspace `root`.
async fn profile_home(root: &Path, name: &str) -> ConfigHome {
    let profile = Workspace::at(root.to_owned())
        .profile(&ProfileName::parse(name).unwrap())
        .await
        .unwrap();
    ConfigHome::Profile(profile)
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
            ..TargetArgs::default()
        },
        consumer_group: Some("flag-group".to_owned()),
        data: DataArgs {
            base_data_folder: Some("flag-data".to_owned()),
            ..DataArgs::default()
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

    assert!(message.contains("No event hub given"), "{message}");
}

#[rstest]
#[case::empty("")]
#[case::spaces("   ")]
fn an_empty_consumer_group_is_refused(#[case] group: &str) {
    let args = ReadArgs {
        consumer_group: Some(group.to_owned()),
        ..with_target(developer())
    };

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

    let job = export_job(
        ExportArgs::default(),
        Some(&recalled),
        here(),
        ConfigSource::Loaded,
    )
    .unwrap();

    assert_eq!(job.export, recalled.storage.export);
    assert_eq!(job.paths, DataPaths::new("recalled-data"));
    assert_eq!(job.filters, CaptureFilters::default());
}

#[test]
fn export_flags_beat_the_recalled_export_settings() {
    let recalled = recalled_developer_config();
    let args = ExportArgs {
        data: DataArgs {
            base_data_folder: Some("flag-data".to_owned()),
            ..DataArgs::default()
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

    let job = export_job(args, Some(&recalled), here(), ConfigSource::Loaded).unwrap();

    assert_eq!(job.paths, DataPaths::new("flag-data"));
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
    let job = export_job(ExportArgs::default(), None, here(), ConfigSource::Missing).unwrap();

    assert_eq!(job.export, ExportSettings::default());
    assert_eq!(job.paths, DataPaths::new(".eh-data"));
}

#[tokio::test]
async fn under_a_profile_the_recalled_data_folder_sits_inside_the_profile_folder() {
    let root = tempfile::tempdir().unwrap();
    let home = profile_home(root.path(), "orders").await;
    let recalled = recalled_developer_config();

    let read = job_at(ReadArgs::default(), &recalled, home.clone());
    let export = export_job(
        ExportArgs::default(),
        Some(&recalled),
        home.clone(),
        ConfigSource::Loaded,
    )
    .unwrap();
    let send = send_at(
        send_args(TargetArgs::default()),
        &recalled,
        home,
        &Environment::default(),
    )
    .unwrap();

    let inside = root.path().join("orders").join("recalled-data");
    assert_eq!(read.paths.base(), inside);
    assert_eq!(export.paths.base(), inside);
    assert_eq!(send.paths.base(), inside);
    assert_eq!(read.config.storage.base_data_folder, "recalled-data");
    assert_eq!(send.config.storage.base_data_folder, "recalled-data");
}

/// The recalled config with `folder` as its data folder, export folder, and received message
/// path.
fn recalled_with_folders(folder: &str) -> ToolConfig {
    let mut recalled = recalled_developer_config();
    recalled.storage.base_data_folder = folder.to_owned();
    recalled.storage.export.export_folder = folder.to_owned();
    recalled.storage.disk_write.received_msg_path = folder.to_owned();
    recalled
}

#[tokio::test]
async fn a_profiles_absolute_folders_are_kept() {
    let root = tempfile::tempdir().unwrap();
    let absolute = tempfile::tempdir().unwrap();
    let recalled = recalled_with_folders(absolute.path().to_str().unwrap());
    let home = profile_home(root.path(), "orders").await;

    let read = job_at(ReadArgs::default(), &recalled, home.clone());
    let export = export_job(
        ExportArgs::default(),
        Some(&recalled),
        home.clone(),
        ConfigSource::Loaded,
    )
    .unwrap();
    let send = send_at(
        send_args(TargetArgs::default()),
        &recalled,
        home,
        &Environment::default(),
    )
    .unwrap();

    assert_eq!(read.paths.base(), absolute.path());
    assert_eq!(
        read.config.storage.disk_write.received_msg_path,
        recalled.storage.disk_write.received_msg_path
    );
    assert_eq!(export.paths.base(), absolute.path());
    assert_eq!(
        export.export.export_folder,
        recalled.storage.export.export_folder
    );
    assert_eq!(send.paths.base(), absolute.path());
}

const OUTSIDE_DATA_FOLDER: &str = "config.json names a data folder outside this folder";
const OUTSIDE_EXPORT_FOLDER: &str = "config.json names an export folder outside this folder";
const OUTSIDE_RECEIVED_PATH: &str = "config.json names a received message path outside this folder";

/// `recalled_developer_config` named on the command line, so the send guard passes.
fn named_send_args() -> SendArgs {
    send_args(TargetArgs {
        namespace: Some("recalled.servicebus.windows.net".to_owned()),
        event_hub: Some("recalled-hub".to_owned()),
        ..TargetArgs::default()
    })
}

#[rstest]
#[case::rooted("/elsewhere/data")]
#[case::climbing("../elsewhere")]
fn a_recalled_data_folder_outside_the_current_folder_is_refused_for_every_command(
    #[case] folder: &str,
) {
    let mut recalled = recalled_developer_config();
    recalled.storage.base_data_folder = folder.to_owned();

    let read = error_of(
        ReadArgs::default(),
        Some(&recalled),
        &Environment::default(),
    );
    let export = export_job(
        ExportArgs::default(),
        Some(&recalled),
        here(),
        ConfigSource::Loaded,
    )
    .unwrap_err();
    let send = send(named_send_args(), Some(&recalled), &Environment::default()).unwrap_err();

    assert_eq!(
        read,
        format!(
            "{OUTSIDE_DATA_FOLDER} ('{folder}'). Pass -b FOLDER to choose one for this run, or \
             keep the setup in a profile (--profile NAME)."
        )
    );
    assert_eq!(export.to_string(), read);
    assert_eq!(send.to_string(), read);
}

#[cfg(windows)]
#[test]
fn a_recalled_network_share_is_refused_as_a_data_folder() {
    let mut recalled = recalled_developer_config();
    recalled.storage.base_data_folder = "\\\\host\\share\\data".to_owned();

    let error = error_of(
        ReadArgs::default(),
        Some(&recalled),
        &Environment::default(),
    );

    assert!(error.contains(OUTSIDE_DATA_FOLDER), "{error}");
}

#[test]
fn a_recalled_data_folder_outside_the_current_folder_is_replaced_by_the_typed_one() {
    let recalled = recalled_with_folders("/elsewhere/data");
    let data = || DataArgs {
        base_data_folder: Some("typed-data".to_owned()),
        ..DataArgs::default()
    };
    let mut send_flags = named_send_args();
    send_flags.data = data();

    let read = job(
        ReadArgs {
            data: data(),
            received_msg_path: Some("typed-inbound".to_owned()),
            ..ReadArgs::default()
        },
        Some(&recalled),
        &Environment::default(),
    )
    .unwrap();
    let export = export_job(
        ExportArgs {
            data: data(),
            export_folder: Some("typed-exports".to_owned()),
            ..ExportArgs::default()
        },
        Some(&recalled),
        here(),
        ConfigSource::Loaded,
    )
    .unwrap();
    let send = send(send_flags, Some(&recalled), &Environment::default()).unwrap();

    assert_eq!(read.paths, DataPaths::new("typed-data"));
    assert_eq!(
        read.config.storage.disk_write.received_msg_path,
        "typed-inbound"
    );
    assert_eq!(export.paths, DataPaths::new("typed-data"));
    assert_eq!(export.export.export_folder, "typed-exports");
    assert_eq!(send.paths, DataPaths::new("typed-data"));
}

#[test]
fn a_recalled_export_folder_outside_the_current_folder_is_refused_for_export() {
    let mut recalled = recalled_developer_config();
    recalled.storage.export.export_folder = "/elsewhere/exports".to_owned();

    let error = export_job(
        ExportArgs::default(),
        Some(&recalled),
        here(),
        ConfigSource::Loaded,
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        format!(
            "{OUTSIDE_EXPORT_FOLDER} ('/elsewhere/exports'). Pass --export-folder FOLDER to \
             choose one for this run, or keep the setup in a profile (--profile NAME)."
        )
    );
}

#[test]
fn a_recalled_received_message_path_outside_the_current_folder_is_refused_for_read() {
    let mut recalled = recalled_developer_config();
    recalled.storage.disk_write.received_msg_path = "/elsewhere/inbound".to_owned();

    let error = error_of(
        ReadArgs::default(),
        Some(&recalled),
        &Environment::default(),
    );

    assert_eq!(
        error,
        format!(
            "{OUTSIDE_RECEIVED_PATH} ('/elsewhere/inbound'). Pass --received-msg-path FOLDER to \
             choose one for this run, or keep the setup in a profile (--profile NAME)."
        )
    );
}

#[test]
fn a_typed_folder_outside_the_current_folder_is_used_as_given() {
    let outside = tempfile::tempdir().unwrap();
    let folder = outside.path().to_str().unwrap();
    let recalled = recalled_developer_config();

    let read = job(
        ReadArgs {
            data: DataArgs {
                base_data_folder: Some(folder.to_owned()),
                ..DataArgs::default()
            },
            received_msg_path: Some(folder.to_owned()),
            ..ReadArgs::default()
        },
        Some(&recalled),
        &Environment::default(),
    )
    .unwrap();

    assert_eq!(read.paths.base(), outside.path());
    assert_eq!(read.config.storage.disk_write.received_msg_path, folder);
}

#[test]
fn the_refusal_escapes_a_hostile_recalled_folder() {
    let mut recalled = recalled_developer_config();
    recalled.storage.base_data_folder = "../evil\u{1b}]52;c;x\u{7}".to_owned();

    let error = error_of(
        ReadArgs::default(),
        Some(&recalled),
        &Environment::default(),
    );

    assert!(error.contains(OUTSIDE_DATA_FOLDER), "{error}");
    assert!(!error.contains('\u{1b}'), "{error:?}");
}

const SAVE_OUTSIDE_DATA_FOLDER: &str = "--save-config cannot save a data folder outside this \
     folder ('/elsewhere/data'): a config.json here is recalled only with folders inside it. \
     Keep the setup in a profile (--profile NAME).";

fn saving(target: TargetArgs) -> TargetArgs {
    TargetArgs {
        save_config: true,
        ..target
    }
}

#[test]
fn save_config_into_the_current_folder_refuses_a_typed_data_folder_outside_it() {
    let data = || DataArgs {
        base_data_folder: Some("/elsewhere/data".to_owned()),
        ..DataArgs::default()
    };
    let mut send_flags = send_args(saving(developer()));
    send_flags.data = data();

    let read = error_of(
        ReadArgs {
            target: saving(developer()),
            data: data(),
            ..ReadArgs::default()
        },
        None,
        &Environment::default(),
    );
    let send = send(send_flags, None, &Environment::default()).unwrap_err();

    assert_eq!(read, SAVE_OUTSIDE_DATA_FOLDER);
    assert_eq!(send.to_string(), SAVE_OUTSIDE_DATA_FOLDER);
}

#[test]
fn save_config_into_the_current_folder_refuses_a_typed_received_message_path_outside_it() {
    let error = error_of(
        ReadArgs {
            target: saving(developer()),
            received_msg_path: Some("../inbound".to_owned()),
            ..ReadArgs::default()
        },
        None,
        &Environment::default(),
    );

    assert_eq!(
        error,
        "--save-config cannot save a received message path outside this folder ('../inbound'): \
         a config.json here is recalled only with folders inside it. Keep the setup in a profile \
         (--profile NAME)."
    );
}

#[tokio::test]
async fn save_config_into_a_profile_keeps_a_typed_received_message_path_outside_it() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let folder = outside.path().to_str().unwrap();

    let job = read_job(
        ReadArgs {
            target: saving(developer()),
            received_msg_path: Some(folder.to_owned()),
            ..ReadArgs::default()
        },
        None,
        &Environment::default(),
        false,
        profile_home(root.path(), "orders").await,
        ConfigSource::Missing,
    )
    .unwrap();

    assert!(job.save_config);
    assert_eq!(job.config.storage.disk_write.received_msg_path, folder);
}

fn job_at(args: ReadArgs, recalled: &ToolConfig, home: ConfigHome) -> ReadJob {
    read_job(
        args,
        Some(recalled),
        &Environment::default(),
        false,
        home,
        ConfigSource::Loaded,
    )
    .unwrap()
}

fn dump_filter(values: &[&str]) -> TextFilterArgs {
    TextFilterArgs {
        dump_filter: Some(values.iter().map(|value| (*value).to_owned()).collect()),
        filter_regex: None,
    }
}

fn keywords(values: &[&str]) -> Option<TextFilter> {
    Some(TextFilter::Contains(
        values.iter().map(|value| (*value).to_owned()).collect(),
    ))
}

#[rstest]
#[case::trailing_empty_dropped(&["a", ""], &["a"])]
#[case::spaces_trimmed(&["error", " timeout"], &["error", "timeout"])]
fn dump_filter_keywords_are_trimmed_and_empty_ones_dropped(
    #[case] values: &[&str],
    #[case] expected: &[&str],
) {
    let read = job(
        ReadArgs {
            target: developer(),
            text: dump_filter(values),
            ..ReadArgs::default()
        },
        None,
        &Environment::default(),
    )
    .unwrap();
    let export = export_job(
        ExportArgs {
            text: dump_filter(values),
            ..ExportArgs::default()
        },
        None,
        here(),
        ConfigSource::Missing,
    )
    .unwrap();

    assert_eq!(read.config.capture.filters.text, keywords(expected));
    assert_eq!(export.filters.text, keywords(expected));
}

#[rstest]
#[case::only_empty_items(&["", ""])]
#[case::one_empty_item(&[""])]
#[case::no_items(&[])]
fn a_dump_filter_with_no_keyword_is_refused_for_read_and_export(#[case] values: &[&str]) {
    let read = error_of(
        ReadArgs {
            target: developer(),
            text: dump_filter(values),
            ..ReadArgs::default()
        },
        None,
        &Environment::default(),
    );
    let export = export_job(
        ExportArgs {
            text: dump_filter(values),
            ..ExportArgs::default()
        },
        None,
        here(),
        ConfigSource::Missing,
    )
    .unwrap_err();

    assert!(read.contains(NO_KEYWORD), "{read}");
    assert!(export.to_string().contains(NO_KEYWORD), "{export:#}");
}

#[test]
fn a_dump_filter_with_no_keyword_does_not_fall_back_to_the_recalled_filter() {
    let recalled = recalled_developer_config();

    let error = error_of(
        ReadArgs {
            text: dump_filter(&[""]),
            ..ReadArgs::default()
        },
        Some(&recalled),
        &Environment::default(),
    );

    assert!(error.contains(NO_KEYWORD), "{error}");
}

fn send_args(target: TargetArgs) -> SendArgs {
    SendArgs {
        target,
        data: DataArgs::default(),
        body: Some(ArgText("x".to_owned())),
        body_file: None,
        count: 3,
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

fn send(
    args: SendArgs,
    recalled: Option<&ToolConfig>,
    environment: &Environment,
) -> Result<SendJob> {
    send_job(
        args,
        (b"x".to_vec(), BodySource::Inline),
        None,
        recalled,
        environment,
        here(),
        ConfigSource::Loaded,
    )
}

fn send_at(
    args: SendArgs,
    recalled: &ToolConfig,
    home: ConfigHome,
    environment: &Environment,
) -> Result<SendJob> {
    send_job(
        args,
        (b"x".to_vec(), BodySource::Inline),
        None,
        Some(recalled),
        environment,
        home,
        ConfigSource::Loaded,
    )
}

fn recalled_key_vault_config() -> ToolConfig {
    ToolConfig::new(TargetConfig {
        namespace: None,
        event_hub: Some("recalled-hub".to_owned()),
        consumer_group: "recalled-group".to_owned(),
        auth: AuthMethodConfig::KeyVaultConnectionString {
            vault_url: "https://v.vault.azure.net".to_owned(),
            secret_name: "s".to_owned(),
            credential: CredentialKindConfig::DeveloperTools,
        },
    })
}

fn recalled_connection_string_config() -> ToolConfig {
    ToolConfig::new(TargetConfig {
        namespace: None,
        event_hub: Some("recalled-hub".to_owned()),
        consumer_group: "recalled-group".to_owned(),
        auth: AuthMethodConfig::ConnectionString {
            stored: StoredSecret::LocalDb,
        },
    })
}

#[rstest]
#[case::flag_auth_without_config(developer(), None)]
#[case::flag_auth_over_config(
    TargetArgs { connection_string: secret(CONNECTION_STRING), ..TargetArgs::default() },
    Some(recalled_developer_config())
)]
#[case::recalled_auth_with_flag_names(
    TargetArgs {
        namespace: Some("flag.servicebus.windows.net".to_owned()),
        event_hub: Some("flag-hub".to_owned()),
        ..TargetArgs::default()
    },
    Some(recalled_developer_config())
)]
#[case::environment_connection_string(TargetArgs::default(), None)]
fn send_merges_the_target_exactly_as_read_does(
    #[case] target: TargetArgs,
    #[case] recalled: Option<ToolConfig>,
) {
    let environment = Environment {
        connection_string: secret(ENV_CONNECTION_STRING),
        client_secret: None,
    };
    let read = job(
        with_target(target_copy(&target)),
        recalled.as_ref(),
        &environment,
    )
    .unwrap();
    let send = send(send_args(target), recalled.as_ref(), &environment).unwrap();

    assert_eq!(send.config.target, read.config.target);
    assert_eq!(
        send.secrets
            .connection_string
            .map(|value| value.expose().to_owned()),
        read.secrets
            .connection_string
            .map(|value| value.expose().to_owned())
    );
}

fn target_copy(target: &TargetArgs) -> TargetArgs {
    TargetArgs {
        connection_string: target.connection_string.clone(),
        namespace: target.namespace.clone(),
        event_hub: target.event_hub.clone(),
        auth: target.auth,
        tenant_id: target.tenant_id.clone(),
        client_id: target.client_id.clone(),
        keyvault_url: target.keyvault_url.clone(),
        keyvault_secret: target.keyvault_secret.clone(),
        save_config: target.save_config,
    }
}

#[test]
fn send_keeps_the_recalled_settings_except_the_data_folder() {
    let recalled = recalled_developer_config();
    let mut args = send_args(TargetArgs {
        namespace: Some("recalled.servicebus.windows.net".to_owned()),
        event_hub: Some("recalled-hub".to_owned()),
        ..TargetArgs::default()
    });
    args.data.base_data_folder = Some("flag-data".to_owned());

    let job = send(args, Some(&recalled), &Environment::default()).unwrap();

    assert_eq!(job.config.target.consumer_group, "recalled-group");
    assert_eq!(job.config.capture, recalled.capture);
    assert_eq!(job.config.storage.disk_write, recalled.storage.disk_write);
    assert_eq!(job.config.storage.export, recalled.storage.export);
    assert_eq!(job.config.storage.base_data_folder, "flag-data");
}

#[test]
fn send_takes_the_count_routing_pacing_and_event_from_the_flags() {
    let mut args = send_args(developer());
    args.partition_key = Some("customer-17".to_owned());
    args.content_type = Some("application/json".to_owned());
    args.interval_ms = Some(250);

    let job = send(args, None, &Environment::default()).unwrap();

    assert_eq!(job.plan.count(), 3);
    assert_eq!(job.plan.routing().key(), Some("customer-17"));
    assert_eq!(job.plan.pacing().get(), Some(Duration::from_millis(250)));
    assert_eq!(job.event.body, b"x");
    assert_eq!(job.event.content_type.as_deref(), Some("application/json"));
    assert_eq!(job.body_source, BodySource::Inline);
}

#[rstest]
#[case::trimmed(" application/json ", Some("application/json"))]
#[case::empty("", None)]
#[case::blank("  ", None)]
fn send_content_type_is_trimmed_and_blank_means_none(
    #[case] given: &str,
    #[case] expected: Option<&str>,
) {
    let mut args = send_args(developer());
    args.content_type = Some(given.to_owned());

    let job = send(args, None, &Environment::default()).unwrap();

    assert_eq!(job.event.content_type.as_deref(), expected);
}

#[test]
fn send_takes_the_properties_and_correlation_id_from_the_flags() {
    let mut args = send_args(developer());
    args.properties = vec![
        ArgText("source=load-test".to_owned()),
        ArgText(" run = 7 ".to_owned()),
    ];
    args.correlation_id = Some(ArgText(" trace-1 ".to_owned()));

    let job = send(args, None, &Environment::default()).unwrap();

    assert_eq!(
        job.event.properties.into_iter().collect::<Vec<_>>(),
        [
            ("run".to_owned(), "7".to_owned()),
            ("source".to_owned(), "load-test".to_owned())
        ]
    );
    assert_eq!(job.event.correlation_id.as_deref(), Some("trace-1"));
}

#[rstest]
#[case::empty("")]
#[case::blank("  ")]
fn a_blank_correlation_id_means_none(#[case] given: &str) {
    let mut args = send_args(developer());
    args.correlation_id = Some(ArgText(given.to_owned()));

    let job = send(args, None, &Environment::default()).unwrap();

    assert_eq!(job.event.correlation_id, None);
}

#[rstest]
#[case::no_equals(&["a=b", "secret-value"], "--property 2: no '=' (write it as key=value)")]
#[case::no_key(&["=secret-value"], "--property 1: no key before '='")]
#[case::repeated(
    &["run=secret-one", "x=y", "run=secret-two"],
    "--property 3: the key 'run' is given twice"
)]
fn a_bad_property_fails_naming_its_flag_and_no_value(
    #[case] values: &[&str],
    #[case] expected: &str,
) {
    let mut args = send_args(developer());
    args.properties = values
        .iter()
        .map(|value| ArgText((*value).to_owned()))
        .collect();

    let error = send(args, None, &Environment::default()).unwrap_err();

    let rendered = format!("{error:#}");
    assert_eq!(rendered, expected);
    assert!(!rendered.contains("secret"), "{rendered}");
}

#[test]
fn send_without_an_interval_is_batched() {
    let job = send(send_args(developer()), None, &Environment::default()).unwrap();

    assert_eq!(job.plan.pacing(), Pacing::batched());
    assert_eq!(job.plan.routing().key(), None);
}

#[test]
fn a_blank_partition_key_is_refused() {
    let mut args = send_args(developer());
    args.partition_key = Some(" ".to_owned());

    let error = send(args, None, &Environment::default()).unwrap_err();

    assert!(
        error.to_string().contains("partition key is empty"),
        "{error:#}"
    );
}

#[rstest]
#[case::connection_string_unnamed(recalled_connection_string_config(), TargetArgs::default())]
#[case::connection_string_hub_only(
    recalled_connection_string_config(),
    TargetArgs { event_hub: Some("recalled-hub".to_owned()), ..TargetArgs::default() }
)]
#[case::key_vault_hub_only(
    recalled_key_vault_config(),
    TargetArgs { event_hub: Some("recalled-hub".to_owned()), ..TargetArgs::default() }
)]
#[case::credential_unnamed(recalled_developer_config(), TargetArgs::default())]
#[case::credential_hub_only(
    recalled_developer_config(),
    TargetArgs { event_hub: Some("recalled-hub".to_owned()), ..TargetArgs::default() }
)]
#[case::credential_namespace_only(
    recalled_developer_config(),
    TargetArgs {
        namespace: Some("recalled.servicebus.windows.net".to_owned()),
        ..TargetArgs::default()
    }
)]
fn a_recalled_target_must_be_named_or_confirmed(
    #[case] recalled: ToolConfig,
    #[case] target: TargetArgs,
) {
    let error = send(send_args(target), Some(&recalled), &Environment::default()).unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("Name it with --namespace and -e"),
        "{message}"
    );
    assert!(message.contains("event hub recalled-hub"), "{message}");
    assert!(
        message.contains("taken from this folder's config.json"),
        "{message}"
    );
    assert!(message.contains("or pass --yes"), "{message}");
}

#[test]
fn connection_string_auth_by_flag_without_a_string_still_needs_the_destination_named() {
    let args = send_args(TargetArgs {
        auth: Some(AuthArg::ConnectionString),
        ..TargetArgs::default()
    });

    let error = send(
        args,
        Some(&recalled_connection_string_config()),
        &Environment::default(),
    )
    .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("taken from the connection string saved in the data folder"),
        "{message}"
    );
    assert!(
        message.contains("Name it with --namespace and -e"),
        "{message}"
    );
}

#[rstest]
#[case::connection_string_with_namespace_and_hub(
    recalled_connection_string_config(),
    TargetArgs {
        namespace: Some("orders.servicebus.windows.net".to_owned()),
        event_hub: Some("recalled-hub".to_owned()),
        ..TargetArgs::default()
    },
    false
)]
#[case::credential_with_namespace_and_hub(
    recalled_developer_config(),
    TargetArgs {
        namespace: Some("recalled.servicebus.windows.net".to_owned()),
        event_hub: Some("recalled-hub".to_owned()),
        ..TargetArgs::default()
    },
    false
)]
#[case::connection_string_confirmed(
    recalled_connection_string_config(),
    TargetArgs::default(),
    true
)]
#[case::credential_confirmed(recalled_developer_config(), TargetArgs::default(), true)]
#[case::flag_credential(recalled_developer_config(), developer(), false)]
#[case::flag_connection_string(
    recalled_developer_config(),
    TargetArgs { connection_string: secret(CONNECTION_STRING), ..TargetArgs::default() },
    false
)]
#[case::flag_key_vault(
    recalled_connection_string_config(),
    TargetArgs {
        auth: Some(AuthArg::Developer),
        keyvault_url: Some("https://v.vault.azure.net".to_owned()),
        keyvault_secret: Some("s".to_owned()),
        ..TargetArgs::default()
    },
    false
)]
fn a_named_confirmed_or_flag_target_passes_the_guard(
    #[case] recalled: ToolConfig,
    #[case] target: TargetArgs,
    #[case] yes: bool,
) {
    let mut args = send_args(target);
    args.yes = yes;

    assert!(send(args, Some(&recalled), &Environment::default()).is_ok());
}

fn environment_with(connection_string: &str) -> Environment {
    Environment {
        connection_string: secret(connection_string),
        client_secret: None,
    }
}

/// The namespace and event hub the job's target and given secrets resolve to.
fn resolved_names(job: &SendJob) -> Result<(String, String)> {
    let target = resolve_target_with(&job.config, job.secrets.clone(), None)?;
    target_names(&target).unwrap()
}

#[test]
fn a_connection_string_from_the_environment_names_the_destination() {
    let environment = environment_with(ENV_CONNECTION_STRING_FOR_RECALLED_HUB);

    let job = send(
        send_args(TargetArgs::default()),
        Some(&recalled_connection_string_config()),
        &environment,
    )
    .unwrap();

    assert_eq!(
        job.secrets.connection_string.as_ref().unwrap().expose(),
        ENV_CONNECTION_STRING_FOR_RECALLED_HUB
    );
    assert_eq!(
        resolved_names(&job).unwrap(),
        (
            "orders.servicebus.windows.net".to_owned(),
            "recalled-hub".to_owned()
        )
    );
}

#[test]
fn an_environment_string_naming_another_hub_passes_the_guard_and_cannot_reach_the_recalled_hub() {
    let environment = environment_with(ENV_CONNECTION_STRING_FOR_ANOTHER_HUB);

    let job = send(
        send_args(TargetArgs::default()),
        Some(&recalled_connection_string_config()),
        &environment,
    )
    .unwrap();

    let error = resolved_names(&job).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not match the connection string EntityPath"),
        "{error:#}"
    );
}

#[test]
fn an_environment_string_without_a_hub_does_not_name_a_recalled_hub() {
    let error = send(
        send_args(TargetArgs::default()),
        Some(&recalled_connection_string_config()),
        &environment_with(ENV_CONNECTION_STRING),
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "This run would send to namespace (from the connection string), event hub recalled-hub, \
         taken from this folder's config.json (the connection string in EH_CONNECTION_STRING \
         does not name an event hub by itself). Name the event hub with -e, or pass --yes."
    );
}

#[test]
fn an_environment_string_without_a_hub_sends_to_the_hub_typed_with_e() {
    let args = send_args(TargetArgs {
        event_hub: Some("typed-hub".to_owned()),
        ..TargetArgs::default()
    });

    let job = send(
        args,
        Some(&recalled_connection_string_config()),
        &environment_with(ENV_CONNECTION_STRING),
    )
    .unwrap();

    assert_eq!(job.config.target.event_hub.as_deref(), Some("typed-hub"));
}

#[rstest]
#[case::not_a_connection_string("marker-7731-not-a-connection-string")]
#[case::foreign_endpoint(
    "Endpoint=sb://marker-7731.example.com/;SharedAccessKeyName=k;SharedAccessKey=eA==;EntityPath=orders"
)]
fn an_environment_string_that_cannot_be_resolved_is_refused_without_quoting_it(
    #[case] connection_string: &str,
) {
    let error = send(
        send_args(TargetArgs::default()),
        Some(&recalled_connection_string_config()),
        &environment_with(connection_string),
    )
    .unwrap_err();

    let message = format!("{error:#}");
    assert!(
        message.contains("does not name an event hub by itself"),
        "{message}"
    );
    assert!(!message.contains("marker-7731"), "{message}");
}

#[test]
fn an_environment_string_without_a_hub_sends_once_confirmed() {
    let mut args = send_args(TargetArgs::default());
    args.yes = true;

    let sent = send(
        args,
        Some(&recalled_connection_string_config()),
        &environment_with(ENV_CONNECTION_STRING),
    );

    assert!(sent.is_ok(), "{sent:?}");
}

#[test]
fn an_auth_flag_with_an_environment_string_passes_the_guard_with_no_recalled_name() {
    let args = send_args(TargetArgs {
        auth: Some(AuthArg::ConnectionString),
        ..TargetArgs::default()
    });

    let job = send(
        args,
        Some(&recalled_connection_string_config()),
        &environment_with(ENV_CONNECTION_STRING),
    )
    .unwrap();

    assert_eq!(job.config.target.namespace, None);
    assert_eq!(job.config.target.event_hub, None);
}

#[tokio::test]
async fn a_profile_that_names_only_its_hub_is_refused_under_an_environment_string_without_a_hub() {
    let root = tempfile::tempdir().unwrap();

    let error = send_at(
        send_args(TargetArgs::default()),
        &recalled_connection_string_config(),
        profile_home(root.path(), "orders").await,
        &environment_with(ENV_CONNECTION_STRING),
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "This run would send to namespace (from the connection string), event hub recalled-hub, \
         taken from profile orders (the connection string in EH_CONNECTION_STRING does not name \
         an event hub by itself). Name the event hub with -e, or pass --yes."
    );
}

#[tokio::test]
async fn a_profile_that_names_both_still_sends_under_an_environment_string_without_a_hub() {
    let root = tempfile::tempdir().unwrap();
    let mut recalled = recalled_connection_string_config();
    recalled.target.namespace = Some("orders.servicebus.windows.net".to_owned());

    let sent = send_at(
        send_args(TargetArgs::default()),
        &recalled,
        profile_home(root.path(), "orders").await,
        &environment_with(ENV_CONNECTION_STRING),
    );

    assert!(sent.is_ok(), "{sent:?}");
}

#[test]
fn the_guard_escapes_a_hostile_recalled_namespace() {
    let mut recalled = recalled_developer_config();
    recalled.target.namespace = Some("evil\u{1b}]52;c;x\u{7}".to_owned());

    let error = send(
        send_args(TargetArgs::default()),
        Some(&recalled),
        &Environment::default(),
    )
    .unwrap_err();

    assert!(!error.to_string().contains('\u{1b}'), "{error:?}");
}

#[test]
fn debug_of_the_send_job_never_shows_a_secret() {
    let args = send_args(TargetArgs {
        connection_string: secret(CONNECTION_STRING),
        ..TargetArgs::default()
    });
    let rendered_args = format!("{args:?}");

    let job = send(args, None, &Environment::default()).unwrap();

    for rendered in [rendered_args, format!("{job:?}")] {
        assert!(!rendered.contains("SharedAccessKey"), "{rendered}");
    }
}

#[tokio::test]
async fn a_profile_that_names_its_namespace_and_hub_sends_without_naming_them_again() {
    let root = tempfile::tempdir().unwrap();
    let mut recalled = recalled_connection_string_config();
    recalled.target.namespace = Some("orders.servicebus.windows.net".to_owned());

    let job = send_at(
        send_args(TargetArgs::default()),
        &recalled,
        profile_home(root.path(), "orders").await,
        &Environment::default(),
    )
    .unwrap();

    assert_eq!(job.config.target, recalled.target);
}

#[tokio::test]
async fn a_profile_that_names_only_its_hub_is_refused_naming_the_profile() {
    let root = tempfile::tempdir().unwrap();

    let error = send_at(
        send_args(TargetArgs::default()),
        &recalled_connection_string_config(),
        profile_home(root.path(), "orders").await,
        &Environment::default(),
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "This run would send to namespace (from the connection string), event hub recalled-hub, \
         taken from profile orders. Name it with --namespace and -e, or pass --yes."
    );
}

#[tokio::test]
async fn an_auth_flag_that_drops_a_profiles_names_still_needs_the_destination_named() {
    let root = tempfile::tempdir().unwrap();
    let mut recalled = recalled_connection_string_config();
    recalled.target.namespace = Some("orders.servicebus.windows.net".to_owned());
    let args = send_args(TargetArgs {
        auth: Some(AuthArg::ConnectionString),
        ..TargetArgs::default()
    });

    let error = send_at(
        args,
        &recalled,
        profile_home(root.path(), "orders").await,
        &Environment::default(),
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "This run would send to namespace (from the connection string), event hub (from the \
         connection string), taken from the connection string saved in the data folder. Name it \
         with --namespace and -e, or pass --yes."
    );
}

#[rstest]
#[case::named(
    TargetArgs {
        namespace: Some("orders.servicebus.windows.net".to_owned()),
        event_hub: Some("recalled-hub".to_owned()),
        ..TargetArgs::default()
    },
    false
)]
#[case::confirmed(TargetArgs::default(), true)]
#[tokio::test]
async fn a_profile_that_names_only_its_hub_sends_once_named_or_confirmed(
    #[case] target: TargetArgs,
    #[case] yes: bool,
) {
    let root = tempfile::tempdir().unwrap();
    let mut args = send_args(target);
    args.yes = yes;

    let sent = send_at(
        args,
        &recalled_connection_string_config(),
        profile_home(root.path(), "orders").await,
        &Environment::default(),
    );

    assert!(sent.is_ok(), "{sent:?}");
}

#[test]
fn parse_regex_rejects_an_invalid_pattern() {
    let error = parse_regex("(unclosed").unwrap_err();

    assert!(error.contains("Invalid text filter regex"), "{error}");
}
