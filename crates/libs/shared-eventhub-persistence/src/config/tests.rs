use super::*;
use crate::paths::DataPaths;

const CONNECTION_STRING: &str = "Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKeyName=listen;SharedAccessKey=c2VjcmV0LWtleQ==";
const CLIENT_SECRET: &str = "client-secret-value-never-on-disk";

struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
        }
    }

    fn dir(&self) -> &Path {
        self.root.path()
    }

    fn config_path(&self) -> std::path::PathBuf {
        self.dir().join(CONFIG_FILE_NAME)
    }

    fn secrets(&self) -> SecretsStore {
        SecretsStore::open(&DataPaths::new(self.dir().join(".eh-data"))).unwrap()
    }
}

fn connection_string_config() -> ToolConfig {
    ToolConfig::new(TargetConfig {
        namespace: None,
        event_hub: Some("orders".to_owned()),
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        auth: AuthMethodConfig::ConnectionString {
            stored: StoredSecret::LocalDb,
        },
    })
}

fn client_secret_kind() -> CredentialKindConfig {
    CredentialKindConfig::ClientSecret {
        tenant_id: "tenant".to_owned(),
        client_id: "client".to_owned(),
    }
}

fn key_vault_config(credential: CredentialKindConfig) -> ToolConfig {
    ToolConfig::new(TargetConfig {
        namespace: None,
        event_hub: None,
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        auth: AuthMethodConfig::KeyVaultConnectionString {
            vault_url: "https://my-vault.vault.azure.net".to_owned(),
            secret_name: "eh-connection".to_owned(),
            credential,
        },
    })
}

fn with_connection_string() -> SensitiveValues {
    SensitiveValues {
        connection_string: Some(CONNECTION_STRING.to_owned().into()),
        client_secret: None,
    }
}

fn assert_no_secret_material(error: &anyhow::Error) {
    for rendered in [
        format!("{error}"),
        format!("{error:#}"),
        format!("{error:?}"),
    ] {
        assert!(!rendered.contains("SharedAccessKey"), "leaked: {rendered}");
        assert!(!rendered.contains("c2VjcmV0LWtleQ=="), "leaked: {rendered}");
    }
}

fn data_folder_contains(fixture: &Fixture, needle: &str) -> bool {
    walk(fixture.dir())
        .into_iter()
        .any(|file| String::from_utf8_lossy(&std::fs::read(file).unwrap()).contains(needle))
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}

#[tokio::test]
async fn a_saved_config_loads_back_equal() {
    let fixture = Fixture::new();
    let mut config = connection_string_config();
    config.capture.filters = CaptureFilters {
        time: None,
        text: Some(shared_eventhub_core::filters::TextFilter::regex("order-\\d+").unwrap()),
    };
    config.storage.disk_write.mode = DiskWriteMode::AtEnd;

    save_config(
        fixture.dir(),
        &config,
        &fixture.secrets(),
        &with_connection_string(),
    )
    .unwrap();
    let loaded = load_config(fixture.dir()).await.unwrap();

    assert_eq!(loaded, ConfigLoad::Loaded(Box::new(config)));
}

#[tokio::test]
async fn the_connection_string_goes_to_the_secrets_file_and_only_a_marker_to_config_json() {
    let fixture = Fixture::new();
    let secrets = fixture.secrets();

    save_config(
        fixture.dir(),
        &connection_string_config(),
        &secrets,
        &with_connection_string(),
    )
    .unwrap();

    let saved = std::fs::read_to_string(fixture.config_path()).unwrap();
    assert!(!saved.contains("SharedAccessKey"), "{saved}");
    assert!(saved.contains("\"stored\": \"local-db\""), "{saved}");
    assert!(saved.ends_with('\n'));
    assert_eq!(
        secrets
            .get(CONNECTION_STRING_SECRET)
            .unwrap()
            .unwrap()
            .expose(),
        CONNECTION_STRING
    );
}

#[tokio::test]
async fn a_missing_config_json_is_missing() {
    let fixture = Fixture::new();

    assert_eq!(
        load_config(fixture.dir()).await.unwrap(),
        ConfigLoad::Missing
    );
}

#[rstest::rstest]
#[case::other_tool("{\"tool\": \"other\", \"port\": 8080}")]
#[case::no_marker("{\"port\": 8080}")]
#[case::jsonc("{\n  // comment\n  \"tool\": \"eh\"\n}")]
#[case::not_an_object("[1, 2, 3]")]
#[tokio::test]
async fn a_config_json_that_is_not_ours_is_foreign_and_never_overwritten(#[case] existing: &str) {
    let fixture = Fixture::new();
    std::fs::write(fixture.config_path(), existing).unwrap();

    let loaded = load_config(fixture.dir()).await.unwrap();
    let saved = save_config(
        fixture.dir(),
        &connection_string_config(),
        &fixture.secrets(),
        &with_connection_string(),
    );

    assert_eq!(loaded, ConfigLoad::Foreign);
    assert!(
        saved
            .unwrap_err()
            .to_string()
            .contains("belongs to another tool")
    );
    assert_eq!(
        std::fs::read_to_string(fixture.config_path()).unwrap(),
        existing
    );
}

#[tokio::test]
async fn a_config_json_that_cannot_be_read_fails() {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.config_path()).unwrap();

    let error = load_config(fixture.dir()).await.unwrap_err();

    assert!(error.to_string().contains("Failed to read"), "{error:#}");
}

#[tokio::test]
async fn an_unknown_field_fails_naming_the_file_and_the_valid_fields_but_not_the_key() {
    let fixture = Fixture::new();
    std::fs::write(
        fixture.config_path(),
        r#"{"tool":"eh","schema_version":1,"surprise":true,"target":{"auth":{"method":"credential","credential":{"kind":"developer-tools"}}}}"#,
    )
    .unwrap();

    let error = load_config(fixture.dir()).await.unwrap_err();

    let rendered = format!("{error:#}");
    assert!(rendered.contains("config.json"), "{rendered}");
    assert!(
        rendered.contains("unknown field `<redacted>`"),
        "{rendered}"
    );
    assert!(rendered.contains("`target`"), "{rendered}");
    assert!(!rendered.contains("surprise"), "{rendered}");
}

#[rstest::rstest]
#[case::as_the_auth_method(format!(
    r#"{{"tool":"eh","schema_version":1,"target":{{"auth":{{"method":"{CONNECTION_STRING}"}}}}}}"#
))]
#[case::as_the_credential_kind(format!(
    r#"{{"tool":"eh","schema_version":1,"target":{{"auth":{{"method":"credential","credential":{{"kind":"{CONNECTION_STRING}"}}}}}}}}"#
))]
#[case::as_the_export_format(format!(
    r#"{{"tool":"eh","schema_version":1,"target":{{"auth":{{"method":"credential","credential":{{"kind":"developer-tools"}}}}}},"storage":{{"export":{{"format":"{CONNECTION_STRING}"}}}}}}"#
))]
#[case::as_the_disk_write_mode(format!(
    r#"{{"tool":"eh","schema_version":1,"target":{{"auth":{{"method":"credential","credential":{{"kind":"developer-tools"}}}}}},"storage":{{"disk_write":{{"mode":"{CONNECTION_STRING}"}}}}}}"#
))]
#[tokio::test]
async fn a_secret_pasted_as_an_enum_value_is_rejected_without_being_echoed(
    #[case] document: String,
) {
    let fixture = Fixture::new();
    std::fs::write(fixture.config_path(), document).unwrap();

    let error = load_config(fixture.dir()).await.unwrap_err();

    assert!(
        format!("{error:#}").contains("unknown variant `<redacted>`"),
        "{error:#}"
    );
    assert_no_secret_material(&error);
}

#[rstest::rstest]
#[case::containing_expected(
    "Endpoint=sb://unexpected-ns.servicebus.windows.net/;SharedAccessKey=c2VjcmV0LWtleQ=="
)]
#[case::containing_a_backtick("a`b;SharedAccessKey=c2VjcmV0LWtleQ==")]
#[case::containing_the_boundary("x`, expected y;SharedAccessKey=c2VjcmV0LWtleQ==")]
#[tokio::test]
async fn a_pasted_secret_shaped_like_the_message_is_still_redacted(#[case] pasted: &str) {
    let fixture = Fixture::new();
    std::fs::write(
        fixture.config_path(),
        format!(
            r#"{{"tool":"eh","schema_version":1,"target":{{"auth":{{"method":"{pasted}"}}}}}}"#
        ),
    )
    .unwrap();

    let error = load_config(fixture.dir()).await.unwrap_err();

    let rendered = format!("{error:#}");
    assert!(
        rendered.contains("unknown variant `<redacted>`, expected one of `connection-string`"),
        "{rendered}"
    );
    assert_no_secret_material(&error);
}

#[rstest::rstest]
#[case::no_boundary(
    "unknown variant `Endpoint=sb://x/;SharedAccessKey=k at line 1",
    "unknown variant `<redacted>`"
)]
#[case::not_an_unknown_name("missing field `target` at line 1", "missing field `target` at line 1")]
fn redact_unknown_name_handles_messages_without_a_boundary(
    #[case] message: &str,
    #[case] expected: &str,
) {
    assert_eq!(redact_unknown_name(message), expected);
}

#[tokio::test]
async fn another_schema_version_fails() {
    let fixture = Fixture::new();
    std::fs::write(
        fixture.config_path(),
        r#"{"tool":"eh","schema_version":2,"target":{"auth":{"method":"credential","credential":{"kind":"managed-identity"}}}}"#,
    )
    .unwrap();

    let error = load_config(fixture.dir()).await.unwrap_err();

    assert!(error.to_string().contains("schema version 2"), "{error:#}");
}

#[tokio::test]
async fn a_secret_pasted_into_stored_is_rejected_without_being_echoed() {
    let fixture = Fixture::new();
    let document = format!(
        r#"{{"tool":"eh","schema_version":1,"target":{{"auth":{{"method":"connection-string","stored":"{CONNECTION_STRING}"}}}}}}"#
    );
    std::fs::write(fixture.config_path(), document).unwrap();

    let error = load_config(fixture.dir()).await.unwrap_err();

    assert!(
        format!("{error:#}").contains("must be `local-db`"),
        "{error:#}"
    );
    assert_no_secret_material(&error);
}

#[rstest::rstest]
#[case::as_the_auth_value(format!(
    r#"{{"tool":"eh","schema_version":1,"target":{{"auth":"{CONNECTION_STRING}"}}}}"#
))]
#[case::as_the_credential(format!(
    r#"{{"tool":"eh","schema_version":1,"target":{{"auth":{{"method":"credential","credential":"{CONNECTION_STRING}"}}}}}}"#
))]
#[case::as_the_namespace(format!(
    r#"{{"tool":"eh","schema_version":1,"target":{{"namespace":["{CONNECTION_STRING}"],"auth":{{"method":"credential","credential":{{"kind":"developer-tools"}}}}}}}}"#
))]
#[tokio::test]
async fn a_secret_pasted_anywhere_is_rejected_without_being_echoed(#[case] document: String) {
    let fixture = Fixture::new();
    std::fs::write(fixture.config_path(), document).unwrap();

    let error = load_config(fixture.dir()).await.unwrap_err();

    assert!(format!("{error:#}").contains("line 1"), "{error:#}");
    assert_no_secret_material(&error);
}

#[tokio::test]
async fn a_utf16_config_json_is_foreign_and_never_overwritten() {
    let fixture = Fixture::new();
    let utf16: Vec<u8> = "{\"tool\":\"eh\"}"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(utf16);
    std::fs::write(fixture.config_path(), &bytes).unwrap();

    let loaded = load_config(fixture.dir()).await.unwrap();
    let saved = save_config(
        fixture.dir(),
        &connection_string_config(),
        &fixture.secrets(),
        &with_connection_string(),
    );

    assert_eq!(loaded, ConfigLoad::Foreign);
    assert!(saved.is_err());
    assert_eq!(std::fs::read(fixture.config_path()).unwrap(), bytes);
}

#[test]
fn saving_over_a_config_json_that_cannot_be_read_fails() {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.config_path()).unwrap();

    let error = save_config(
        fixture.dir(),
        &connection_string_config(),
        &fixture.secrets(),
        &with_connection_string(),
    )
    .unwrap_err();

    assert!(
        error.to_string().contains("Failed to read the existing"),
        "{error:#}"
    );
}

#[tokio::test]
async fn a_bom_prefixed_config_loads() {
    let fixture = Fixture::new();
    std::fs::write(
        fixture.config_path(),
        "\u{FEFF}{\"tool\":\"eh\",\"schema_version\":1,\"target\":{\"auth\":{\"method\":\"credential\",\"credential\":{\"kind\":\"developer-tools\"}}}}",
    )
    .unwrap();

    let loaded = load_config(fixture.dir()).await.unwrap();

    assert!(matches!(loaded, ConfigLoad::Loaded(_)));
}

#[test]
fn saving_again_without_a_connection_string_keeps_the_stored_one() {
    let fixture = Fixture::new();
    let secrets = fixture.secrets();
    save_config(
        fixture.dir(),
        &connection_string_config(),
        &secrets,
        &with_connection_string(),
    )
    .unwrap();

    save_config(
        fixture.dir(),
        &connection_string_config(),
        &secrets,
        &SensitiveValues::default(),
    )
    .unwrap();

    assert!(secrets.get(CONNECTION_STRING_SECRET).unwrap().is_some());
}

#[test]
fn saving_connection_string_auth_with_no_connection_string_anywhere_fails() {
    let fixture = Fixture::new();

    let error = save_config(
        fixture.dir(),
        &connection_string_config(),
        &fixture.secrets(),
        &SensitiveValues::default(),
    )
    .unwrap_err();

    assert!(
        error.to_string().contains("no connection string"),
        "{error:#}"
    );
    assert!(!fixture.config_path().exists());
}

#[test]
fn switching_to_key_vault_removes_the_stored_connection_string() {
    let fixture = Fixture::new();
    let secrets = fixture.secrets();
    save_config(
        fixture.dir(),
        &connection_string_config(),
        &secrets,
        &with_connection_string(),
    )
    .unwrap();

    save_config(
        fixture.dir(),
        &key_vault_config(CredentialKindConfig::DeveloperTools),
        &secrets,
        &SensitiveValues::default(),
    )
    .unwrap();

    assert!(secrets.get(CONNECTION_STRING_SECRET).unwrap().is_none());
}

#[test]
fn a_key_vault_save_with_a_connection_string_is_rejected_and_writes_nothing() {
    let fixture = Fixture::new();
    let secrets = fixture.secrets();

    let error = save_config(
        fixture.dir(),
        &key_vault_config(CredentialKindConfig::DeveloperTools),
        &secrets,
        &with_connection_string(),
    )
    .unwrap_err();

    assert!(
        error.to_string().contains("only the vault reference"),
        "{error:#}"
    );
    assert!(!fixture.config_path().exists());
    assert!(secrets.names().unwrap().is_empty());
}

#[test]
fn a_client_secret_is_stored_under_its_client_id_and_never_in_config_json() {
    let fixture = Fixture::new();
    let secrets = fixture.secrets();
    let sensitive = SensitiveValues {
        connection_string: None,
        client_secret: Some(CLIENT_SECRET.to_owned().into()),
    };

    save_config(
        fixture.dir(),
        &key_vault_config(client_secret_kind()),
        &secrets,
        &sensitive,
    )
    .unwrap();

    assert_eq!(
        secrets
            .get("client_secret:client")
            .unwrap()
            .unwrap()
            .expose(),
        CLIENT_SECRET
    );
    assert!(
        !std::fs::read_to_string(fixture.config_path())
            .unwrap()
            .contains(CLIENT_SECRET)
    );
}

#[test]
fn a_client_secret_without_a_client_secret_credential_is_rejected() {
    let fixture = Fixture::new();
    let sensitive = SensitiveValues {
        connection_string: None,
        client_secret: Some(CLIENT_SECRET.to_owned().into()),
    };

    let error = save_config(
        fixture.dir(),
        &key_vault_config(CredentialKindConfig::ManagedIdentity),
        &fixture.secrets(),
        &sensitive,
    )
    .unwrap_err();

    assert!(
        error.to_string().contains("no client-secret credential"),
        "{error:#}"
    );
}

#[test]
fn saving_a_config_without_the_marker_is_refused() {
    let fixture = Fixture::new();
    let mut config = connection_string_config();
    config.tool = "other".to_owned();

    let error = save_config(
        fixture.dir(),
        &config,
        &fixture.secrets(),
        &with_connection_string(),
    )
    .unwrap_err();

    assert!(error.to_string().contains("Refusing to save"), "{error:#}");
}

#[test]
fn a_key_vault_config_never_puts_a_connection_string_on_disk() {
    let fixture = Fixture::new();
    let secrets = fixture.secrets();

    save_config(
        fixture.dir(),
        &key_vault_config(CredentialKindConfig::DeveloperTools),
        &secrets,
        &SensitiveValues::default(),
    )
    .unwrap();
    drop(secrets);

    assert!(!data_folder_contains(&fixture, "SharedAccessKey"));
}

#[test]
fn resolve_target_rebuilds_a_connection_string_target() {
    let fixture = Fixture::new();
    let secrets = fixture.secrets();
    save_config(
        fixture.dir(),
        &connection_string_config(),
        &secrets,
        &with_connection_string(),
    )
    .unwrap();

    let target = resolve_target(&connection_string_config(), &secrets).unwrap();

    assert!(matches!(
        &target.auth,
        AuthMethod::ConnectionString { connection_string } if connection_string.expose() == CONNECTION_STRING
    ));
    assert_eq!(target.event_hub_name.as_deref(), Some("orders"));
    assert_eq!(target.consumer_group, DEFAULT_CONSUMER_GROUP);
}

#[test]
fn resolve_target_rebuilds_a_key_vault_target_with_its_client_secret() {
    let fixture = Fixture::new();
    let secrets = fixture.secrets();
    secrets
        .put("client_secret:client", &CLIENT_SECRET.to_owned().into())
        .unwrap();

    let target = resolve_target(&key_vault_config(client_secret_kind()), &secrets).unwrap();

    assert!(matches!(
        &target.auth,
        AuthMethod::KeyVaultConnectionString {
            secret_name,
            credential: CredentialKind::ClientSecret { client_id, .. },
            ..
        } if secret_name == "eh-connection" && client_id == "client"
    ));
    assert_eq!(target.client_secret.unwrap().expose(), CLIENT_SECRET);
}

#[test]
fn resolve_target_rebuilds_a_credential_target() {
    let fixture = Fixture::new();
    let config = ToolConfig::new(TargetConfig {
        namespace: Some("orders.servicebus.windows.net".to_owned()),
        event_hub: Some("orders".to_owned()),
        consumer_group: "eh-tool".to_owned(),
        auth: AuthMethodConfig::Credential {
            credential: CredentialKindConfig::ManagedIdentity,
        },
    });

    let target = resolve_target(&config, &fixture.secrets()).unwrap();

    assert!(matches!(
        target.auth,
        AuthMethod::Credential {
            kind: CredentialKind::ManagedIdentity
        }
    ));
    assert!(target.client_secret.is_none());
    assert_eq!(target.consumer_group, "eh-tool");
}

#[test]
fn resolve_target_without_the_stored_connection_string_fails_readably() {
    let fixture = Fixture::new();

    let error = resolve_target(&connection_string_config(), &fixture.secrets()).unwrap_err();

    assert!(error.to_string().contains("secrets file"), "{error:#}");
}

#[test]
fn resolve_target_with_prefers_a_given_connection_string_over_the_stored_one() {
    let fixture = Fixture::new();
    let secrets = fixture.secrets();
    secrets
        .put(CONNECTION_STRING_SECRET, &"stored".to_owned().into())
        .unwrap();

    let target = resolve_target_with(
        &connection_string_config(),
        with_connection_string(),
        Some(&secrets),
    )
    .unwrap();

    assert!(matches!(
        &target.auth,
        AuthMethod::ConnectionString { connection_string } if connection_string.expose() == CONNECTION_STRING
    ));
}

#[test]
fn resolve_target_with_reads_a_missing_client_secret_from_the_store() {
    let fixture = Fixture::new();
    let secrets = fixture.secrets();
    secrets
        .put(
            &client_secret_name("client"),
            &CLIENT_SECRET.to_owned().into(),
        )
        .unwrap();
    let config = ToolConfig::new(TargetConfig {
        namespace: Some("orders.servicebus.windows.net".to_owned()),
        event_hub: Some("orders".to_owned()),
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        auth: AuthMethodConfig::Credential {
            credential: client_secret_kind(),
        },
    });

    let target = resolve_target_with(&config, SensitiveValues::default(), Some(&secrets)).unwrap();

    assert_eq!(target.client_secret.unwrap().expose(), CLIENT_SECRET);
}

#[test]
fn resolve_target_with_no_store_and_no_given_connection_string_fails() {
    let error = resolve_target_with(
        &connection_string_config(),
        SensitiveValues::default(),
        None,
    )
    .unwrap_err();

    assert!(error.to_string().contains("secrets file"), "{error:#}");
}

#[rstest::rstest]
#[case::connection_string_not_given(connection_string_config(), SensitiveValues::default(), true)]
#[case::connection_string_given(connection_string_config(), with_connection_string(), false)]
#[case::client_secret_not_given(
    key_vault_config(client_secret_kind()),
    SensitiveValues::default(),
    true
)]
#[case::client_secret_given(
    key_vault_config(client_secret_kind()),
    SensitiveValues { connection_string: None, client_secret: Some(CLIENT_SECRET.to_owned().into()) },
    false
)]
#[case::no_secret_used(
    key_vault_config(CredentialKindConfig::DeveloperTools),
    SensitiveValues::default(),
    false
)]
fn needs_stored_secret_is_true_only_for_a_used_secret_that_was_not_given(
    #[case] config: ToolConfig,
    #[case] given: SensitiveValues,
    #[case] expected: bool,
) {
    assert_eq!(needs_stored_secret(&config, &given), expected);
}

fn credential_config(credential: CredentialKindConfig) -> ToolConfig {
    ToolConfig::new(TargetConfig {
        namespace: Some("orders.servicebus.windows.net".to_owned()),
        event_hub: Some("orders".to_owned()),
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        auth: AuthMethodConfig::Credential { credential },
    })
}

#[rstest::rstest]
#[case::connection_string(connection_string_config(), with_connection_string(), None)]
#[case::developer_tools(
    credential_config(CredentialKindConfig::DeveloperTools),
    SensitiveValues::default(),
    None
)]
#[case::client_secret_given(
    credential_config(client_secret_kind()),
    SensitiveValues { connection_string: None, client_secret: Some(CLIENT_SECRET.to_owned().into()) },
    None
)]
#[case::client_secret_missing(
    credential_config(client_secret_kind()),
    SensitiveValues::default(),
    Some("client")
)]
fn missing_client_secret_names_the_client_only_when_its_secret_was_not_resolved(
    #[case] config: ToolConfig,
    #[case] given: SensitiveValues,
    #[case] expected: Option<&str>,
) {
    let target = resolve_target_with(&config, given, None).unwrap();

    assert_eq!(config.missing_client_secret(&target), expected);
}

#[test]
fn disk_write_config_places_the_inbound_folder_in_the_data_folder() {
    let mut config = connection_string_config();
    config.storage.disk_write = DiskWriteSettings {
        mode: DiskWriteMode::AtEnd,
        received_msg_path: "inbound".to_owned(),
        content_only: true,
    };
    let paths = DataPaths::new("data");

    let disk_write = config.disk_write_config(&paths);

    assert_eq!(disk_write.mode, DiskWriteMode::AtEnd);
    assert_eq!(disk_write.inbound_dir, Path::new("data").join("inbound"));
    assert!(disk_write.content_only);
}

#[test]
fn engine_config_carries_the_capture_settings_and_the_target() {
    let mut config = connection_string_config();
    config.capture.partitions = Some(vec!["1".to_owned()]);
    config.capture.ignore_checkpoint = true;
    config.capture.prefetch = Some(64);
    config.capture.feedback_interval_secs = 3;
    let target = resolve_target_with(
        &config,
        SensitiveValues {
            connection_string: Some(SecretString::from(CONNECTION_STRING.to_owned())),
            client_secret: None,
        },
        None,
    )
    .unwrap();

    let engine = config.engine_config(target);

    assert_eq!(engine.partitions, Some(vec!["1".to_owned()]));
    assert!(engine.ignore_checkpoint);
    assert_eq!(engine.prefetch, Some(64));
    assert_eq!(engine.feedback_interval, Duration::from_secs(3));
    assert_eq!(engine.target.event_hub_name.as_deref(), Some("orders"));
}

const WITH_ENTITY_PATH: &str = "Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKeyName=listen;SharedAccessKey=c2VjcmV0LWtleQ==;EntityPath=refunds";

fn given_string(connection_string: &str) -> SensitiveValues {
    SensitiveValues {
        connection_string: Some(connection_string.to_owned().into()),
        client_secret: None,
    }
}

fn names(config: &ToolConfig) -> (Option<&str>, Option<&str>) {
    (
        config.target.namespace.as_deref(),
        config.target.event_hub.as_deref(),
    )
}

#[test]
fn with_typed_names_fills_both_names_from_a_string_with_an_entity_path() {
    let mut config = connection_string_config();
    config.target.event_hub = None;

    let named = with_typed_names(&config, &given_string(WITH_ENTITY_PATH)).unwrap();

    assert!(matches!(named, Cow::Owned(_)));
    assert_eq!(
        names(&named),
        (Some("orders.servicebus.windows.net"), Some("refunds"))
    );
}

#[test]
fn with_typed_names_fills_only_the_hub_when_the_namespace_is_set() {
    let mut config = connection_string_config();
    config.target.namespace = Some("orders.servicebus.windows.net".to_owned());
    config.target.event_hub = None;

    let named = with_typed_names(&config, &given_string(WITH_ENTITY_PATH)).unwrap();

    assert_eq!(
        names(&named),
        (Some("orders.servicebus.windows.net"), Some("refunds"))
    );
}

#[test]
fn with_typed_names_keeps_a_configured_hub_and_fills_the_namespace() {
    let config = connection_string_config();

    let named = with_typed_names(&config, &with_connection_string()).unwrap();

    assert_eq!(
        names(&named),
        (Some("orders.servicebus.windows.net"), Some("orders"))
    );
}

fn both_named(auth: AuthMethodConfig) -> ToolConfig {
    ToolConfig::new(TargetConfig {
        namespace: Some("orders.servicebus.windows.net".to_owned()),
        event_hub: Some("orders".to_owned()),
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        auth,
    })
}

#[rstest::rstest]
#[case::both_names_set(
    both_named(AuthMethodConfig::ConnectionString { stored: StoredSecret::LocalDb }),
    given_string(WITH_ENTITY_PATH)
)]
#[case::no_string_given(connection_string_config(), SensitiveValues::default())]
#[case::credential_auth(
    credential_config(CredentialKindConfig::DeveloperTools),
    given_string(WITH_ENTITY_PATH)
)]
#[case::key_vault_auth(
    key_vault_config(CredentialKindConfig::DeveloperTools),
    given_string(WITH_ENTITY_PATH)
)]
fn with_typed_names_borrows_the_config_when_there_is_nothing_to_fill(
    #[case] config: ToolConfig,
    #[case] given: SensitiveValues,
) {
    let named = with_typed_names(&config, &given).unwrap();

    assert!(matches!(named, Cow::Borrowed(_)));
    assert_eq!(*named, config);
}

#[test]
fn with_typed_names_fails_on_another_namespace_without_echoing_the_string() {
    let mut config = connection_string_config();
    config.target.namespace = Some("billing.servicebus.windows.net".to_owned());
    config.target.event_hub = None;

    let error = with_typed_names(&config, &given_string(WITH_ENTITY_PATH)).unwrap_err();

    assert!(error.to_string().contains("does not match"), "{error:#}");
    assert_no_secret_material(&error);
}

#[test]
fn with_typed_names_fails_when_no_hub_is_named_anywhere() {
    let mut config = connection_string_config();
    config.target.event_hub = None;

    let error = with_typed_names(&config, &with_connection_string()).unwrap_err();

    assert!(
        format!("{error:#}").contains("An event hub name is required"),
        "{error:#}"
    );
    assert_no_secret_material(&error);
}

#[test]
fn with_typed_names_fails_on_a_string_that_does_not_parse_without_echoing_it() {
    let mut config = connection_string_config();
    config.target.event_hub = None;
    let malformed =
        "Endpoint=not a url;SharedAccessKeyName=listen;SharedAccessKey=c2VjcmV0LWtleQ==";

    let error = with_typed_names(&config, &given_string(malformed)).unwrap_err();

    assert!(
        format!("{error:#}").contains("Invalid connection string"),
        "{error:#}"
    );
    assert_no_secret_material(&error);
}

#[test]
fn with_stored_secrets_reads_the_connection_string_the_config_uses() {
    let fixture = Fixture::new();
    let secrets = fixture.secrets();
    secrets
        .put(
            CONNECTION_STRING_SECRET,
            &CONNECTION_STRING.to_owned().into(),
        )
        .unwrap();

    let merged = with_stored_secrets(
        &connection_string_config(),
        SensitiveValues::default(),
        &secrets,
    )
    .unwrap();

    assert_eq!(
        merged
            .connection_string
            .map(|value| value.expose().to_owned()),
        Some(CONNECTION_STRING.to_owned())
    );
    assert!(merged.client_secret.is_none());
}

#[test]
fn with_stored_secrets_reads_the_client_secret_of_the_configs_client() {
    let fixture = Fixture::new();
    let secrets = fixture.secrets();
    secrets
        .put(
            &client_secret_name("client"),
            &CLIENT_SECRET.to_owned().into(),
        )
        .unwrap();

    let merged = with_stored_secrets(
        &credential_config(client_secret_kind()),
        SensitiveValues::default(),
        &secrets,
    )
    .unwrap();

    assert_eq!(
        merged.client_secret.map(|value| value.expose().to_owned()),
        Some(CLIENT_SECRET.to_owned())
    );
    assert!(merged.connection_string.is_none());
}

#[test]
fn with_stored_secrets_keeps_a_given_secret_over_the_stored_one() {
    let fixture = Fixture::new();
    let secrets = fixture.secrets();
    secrets
        .put(CONNECTION_STRING_SECRET, &"stored".to_owned().into())
        .unwrap();

    let merged = with_stored_secrets(
        &connection_string_config(),
        with_connection_string(),
        &secrets,
    )
    .unwrap();

    assert_eq!(
        merged
            .connection_string
            .map(|value| value.expose().to_owned()),
        Some(CONNECTION_STRING.to_owned())
    );
}

#[rstest::rstest]
#[case::connection_string(
    connection_string_config(),
    "The connection string is not in the secrets file"
)]
#[case::client_secret(
    credential_config(client_secret_kind()),
    "The client secret for this client id is not in the secrets file"
)]
fn with_stored_secrets_fails_naming_the_kind_when_the_secret_is_nowhere(
    #[case] config: ToolConfig,
    #[case] expected: &str,
) {
    let fixture = Fixture::new();

    let error =
        with_stored_secrets(&config, SensitiveValues::default(), &fixture.secrets()).unwrap_err();

    assert_eq!(error.to_string(), expected);
}
