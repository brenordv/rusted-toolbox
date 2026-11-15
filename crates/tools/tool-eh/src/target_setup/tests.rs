use super::*;
use shared_azure::auth::AuthMethod;
use shared_eventhub_core::connection::DEFAULT_CONSUMER_GROUP;
use shared_eventhub_persistence::config::{
    AuthMethodConfig, CONFIG_FILE_NAME, CredentialKindConfig, StoredSecret, TargetConfig,
};
use shared_eventhub_persistence::secrets::CONNECTION_STRING_SECRET;

const CONNECTION_STRING: &str = "Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKeyName=listen;SharedAccessKey=c2VjcmV0";

struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
        }
    }

    fn paths(&self) -> DataPaths {
        DataPaths::new(self.root.path().join(".eh-data"))
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
        let paths = self.paths();
        prepare_target(
            &TargetSetup {
                config: &config,
                given,
                paths: &paths,
                save_config,
                working_dir: self.root.path(),
            },
            renderer,
        )
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
        saved_notice(&SensitiveValues::default(), &fixture.paths()),
        "Saved config.json"
    );
}
