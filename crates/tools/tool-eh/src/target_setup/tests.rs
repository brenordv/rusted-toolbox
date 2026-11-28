use super::*;
use common_azure::auth::AuthMethod;
use eh_core::connection::DEFAULT_CONSUMER_GROUP;
use eh_persistence::config::{
    AuthMethodConfig, CONFIG_FILE_NAME, ConfigLoad, CredentialKindConfig, StoredSecret,
    TargetConfig, load_config,
};
use eh_persistence::secrets::CONNECTION_STRING_SECRET;
use eh_persistence::workspace::{ProfileName, Workspace};
use std::path::PathBuf;

const CONNECTION_STRING: &str = "Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKeyName=listen;SharedAccessKey=c2VjcmV0";
const STRING_WITH_HUB: &str = "Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKeyName=listen;SharedAccessKey=c2VjcmV0;EntityPath=orders-hub";

struct Fixture {
    root: tempfile::TempDir,
    home: ConfigHome,
    paths: DataPaths,
}

impl Fixture {
    /// A fixture whose home is the temp folder, with the data folder in it.
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = ConfigHome::CurrentFolder(root.path().to_owned());
        let paths = DataPaths::new(root.path().join(".eh-data"));
        Self { root, home, paths }
    }

    /// A fixture whose home is the new profile `orders` in a workspace
    /// inside the temp folder.
    async fn in_profile() -> Self {
        let root = tempfile::tempdir().unwrap();
        let profile = Workspace::at(root.path().join("profiles"))
            .profile(&ProfileName::parse("orders").unwrap())
            .await
            .unwrap();
        let home = ConfigHome::Profile(profile);
        let paths = home.data_paths(".eh-data");
        Self { root, home, paths }
    }

    fn paths(&self) -> DataPaths {
        self.paths.clone()
    }

    fn profile_dir(&self) -> PathBuf {
        self.root.path().join("profiles").join("orders")
    }

    fn prepare(
        &self,
        auth: AuthMethodConfig,
        given: &SensitiveValues,
        save_config: bool,
        renderer: &mut Renderer<Vec<u8>, Vec<u8>>,
    ) -> anyhow::Result<EventHubTarget> {
        let config = ToolConfig::new(TargetConfig {
            namespace: Some("orders.servicebus.windows.net".to_owned()),
            event_hub: Some("orders".to_owned()),
            consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
            auth,
        });
        self.prepare_config(&config, given, save_config, renderer)
    }

    fn prepare_config(
        &self,
        config: &ToolConfig,
        given: &SensitiveValues,
        save_config: bool,
        renderer: &mut Renderer<Vec<u8>, Vec<u8>>,
    ) -> anyhow::Result<EventHubTarget> {
        let paths = self.paths();
        prepare_target(
            &TargetSetup {
                config,
                given,
                paths: &paths,
                save_config,
                home: &self.home,
            },
            renderer,
        )
    }
}

fn given(connection_string: &str) -> SensitiveValues {
    SensitiveValues {
        connection_string: Some(connection_string.to_owned().into()),
        client_secret: None,
    }
}

/// A connection-string config that names neither the namespace nor the
/// event hub.
fn unnamed_connection_string_config() -> ToolConfig {
    ToolConfig::new(TargetConfig {
        namespace: None,
        event_hub: None,
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        auth: connection_string_auth(),
    })
}

async fn saved_target(dir: &std::path::Path) -> TargetConfig {
    match load_config(dir).await.unwrap() {
        ConfigLoad::Loaded(config) => config.target,
        other => panic!("expected a saved config, got {other:?}"),
    }
}

fn renderer() -> Renderer<Vec<u8>, Vec<u8>> {
    Renderer::new(Vec::new(), Vec::new(), false, 200)
}

fn connection_string_given() -> SensitiveValues {
    SensitiveValues {
        connection_string: Some(CONNECTION_STRING.to_owned().into()),
        client_secret: None,
    }
}

fn connection_string_auth() -> AuthMethodConfig {
    AuthMethodConfig::ConnectionString {
        stored: StoredSecret::LocalDb,
    }
}

#[test]
fn a_given_secret_resolves_without_touching_the_data_folder() {
    let fixture = Fixture::new();

    let target = fixture
        .prepare(
            connection_string_auth(),
            &connection_string_given(),
            false,
            &mut renderer(),
        )
        .unwrap();

    assert!(matches!(target.auth, AuthMethod::ConnectionString { .. }));
    assert!(!fixture.paths().base().exists());
}

#[test]
fn save_config_writes_config_json_and_the_secret_and_says_where() {
    let fixture = Fixture::new();
    let mut renderer = renderer();

    fixture
        .prepare(
            connection_string_auth(),
            &connection_string_given(),
            true,
            &mut renderer,
        )
        .unwrap();

    let saved = std::fs::read_to_string(fixture.root.path().join(CONFIG_FILE_NAME)).unwrap();
    assert!(!saved.contains("SharedAccessKey"), "{saved}");
    let secrets = SecretsStore::open(&fixture.paths()).unwrap();
    assert_eq!(
        secrets
            .get(CONNECTION_STRING_SECRET)
            .unwrap()
            .unwrap()
            .expose(),
        CONNECTION_STRING
    );
    let notice = String::from_utf8(renderer.into_parts().1).unwrap();
    assert!(
        notice.contains("Saved config.json; the connection string is stored in"),
        "{notice}"
    );
    assert!(notice.contains("(not encrypted)"), "{notice}");
}

#[test]
fn a_client_secret_neither_given_nor_stored_fails_with_guidance() {
    let fixture = Fixture::new();
    let auth = AuthMethodConfig::Credential {
        credential: CredentialKindConfig::ClientSecret {
            tenant_id: "tenant".to_owned(),
            client_id: "client".to_owned(),
        },
    };

    let error = fixture
        .prepare(auth, &SensitiveValues::default(), false, &mut renderer())
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("No client secret for client 'client'"),
        "{error:#}"
    );
    assert!(error.to_string().contains(CLIENT_SECRET_ENV), "{error:#}");
}

#[test]
fn a_stored_connection_string_is_read_when_none_was_given() {
    let fixture = Fixture::new();
    SecretsStore::open(&fixture.paths())
        .unwrap()
        .put(
            CONNECTION_STRING_SECRET,
            &CONNECTION_STRING.to_owned().into(),
        )
        .unwrap();

    let target = fixture
        .prepare(
            connection_string_auth(),
            &SensitiveValues::default(),
            false,
            &mut renderer(),
        )
        .unwrap();

    assert!(matches!(
        &target.auth,
        AuthMethod::ConnectionString { connection_string } if connection_string.expose() == CONNECTION_STRING
    ));
}

#[test]
fn saving_without_a_secret_says_so_plainly() {
    let fixture = Fixture::new();

    assert_eq!(
        saved_notice(&SensitiveValues::default(), &fixture.paths(), &fixture.home),
        "Saved config.json"
    );
}

#[tokio::test]
async fn saving_a_new_profile_creates_it_and_records_the_strings_names() {
    let fixture = Fixture::in_profile().await;
    let mut renderer = renderer();

    fixture
        .prepare_config(
            &unnamed_connection_string_config(),
            &given(STRING_WITH_HUB),
            true,
            &mut renderer,
        )
        .unwrap();

    let saved = saved_target(&fixture.profile_dir()).await;
    assert_eq!(
        saved.namespace.as_deref(),
        Some("orders.servicebus.windows.net")
    );
    assert_eq!(saved.event_hub.as_deref(), Some("orders-hub"));
    assert!(
        fixture
            .profile_dir()
            .join(".eh-data")
            .join("secrets.redb")
            .exists()
    );
    let notice = String::from_utf8(renderer.into_parts().1).unwrap();
    assert!(
        notice.contains("Saved profile orders; the connection string is stored in"),
        "{notice}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_new_profile_folder_has_no_group_or_other_bits() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::in_profile().await;

    fixture
        .prepare_config(
            &unnamed_connection_string_config(),
            &given(STRING_WITH_HUB),
            true,
            &mut renderer(),
        )
        .unwrap();

    let mode = std::fs::metadata(fixture.profile_dir())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0);
}

#[tokio::test]
async fn a_given_hub_is_kept_and_only_the_missing_namespace_is_filled() {
    let fixture = Fixture::in_profile().await;
    let mut config = unnamed_connection_string_config();
    config.target.event_hub = Some("orders-hub".to_owned());

    fixture
        .prepare_config(&config, &given(STRING_WITH_HUB), true, &mut renderer())
        .unwrap();

    let saved = saved_target(&fixture.profile_dir()).await;
    assert_eq!(
        saved.namespace.as_deref(),
        Some("orders.servicebus.windows.net")
    );
    assert_eq!(saved.event_hub.as_deref(), Some("orders-hub"));
}

#[tokio::test]
async fn re_saving_a_profile_with_only_a_stored_string_leaves_the_names_unfilled() {
    let fixture = Fixture::in_profile().await;
    SecretsStore::open(&fixture.paths())
        .unwrap()
        .put(CONNECTION_STRING_SECRET, &STRING_WITH_HUB.to_owned().into())
        .unwrap();

    fixture
        .prepare_config(
            &unnamed_connection_string_config(),
            &SensitiveValues::default(),
            true,
            &mut renderer(),
        )
        .unwrap();

    let saved = saved_target(&fixture.profile_dir()).await;
    assert_eq!(saved.namespace, None);
    assert_eq!(saved.event_hub, None);
}

#[tokio::test]
async fn a_key_vault_profile_is_saved_without_names() {
    let fixture = Fixture::in_profile().await;
    let config = ToolConfig::new(TargetConfig {
        namespace: None,
        event_hub: None,
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        auth: AuthMethodConfig::KeyVaultConnectionString {
            vault_url: "https://v.vault.azure.net".to_owned(),
            secret_name: "s".to_owned(),
            credential: CredentialKindConfig::DeveloperTools,
        },
    });

    fixture
        .prepare_config(&config, &SensitiveValues::default(), true, &mut renderer())
        .unwrap();

    let saved = saved_target(&fixture.profile_dir()).await;
    assert_eq!(saved.namespace, None);
    assert_eq!(saved.event_hub, None);
}

#[tokio::test]
async fn a_string_that_names_no_hub_fails_before_anything_is_written() {
    let fixture = Fixture::in_profile().await;

    let error = fixture
        .prepare_config(
            &unnamed_connection_string_config(),
            &given(CONNECTION_STRING),
            true,
            &mut renderer(),
        )
        .unwrap_err();

    assert!(!format!("{error:#}").contains("c2VjcmV0"), "{error:#}");
    assert!(!fixture.profile_dir().exists());
}

#[tokio::test]
async fn saving_into_the_current_folder_never_fills_the_names() {
    let fixture = Fixture::new();

    fixture
        .prepare_config(
            &unnamed_connection_string_config(),
            &given(STRING_WITH_HUB),
            true,
            &mut renderer(),
        )
        .unwrap();

    let saved = saved_target(fixture.root.path()).await;
    assert_eq!(saved.namespace, None);
    assert_eq!(saved.event_hub, None);
}
