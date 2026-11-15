use super::*;
use common_utils::string_utils::SecretString;
use shared_azure::auth::AuthMethod;
use shared_eventhub_core::connection::DEFAULT_CONSUMER_GROUP;
use shared_eventhub_persistence::config::{
    AuthMethodConfig, CredentialKindConfig, StoredSecret, TargetConfig,
};
use shared_eventhub_persistence::secrets::{CONNECTION_STRING_SECRET, client_secret_name};

const CONNECTION_STRING: &str = "Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKeyName=listen;SharedAccessKey=c2VjcmV0;EntityPath=orders";

fn request(
    auth: AuthMethodConfig,
    given: SensitiveValues,
    root: &tempfile::TempDir,
) -> ConnectRequest {
    ConnectRequest {
        config: ToolConfig::new(TargetConfig {
            namespace: Some("orders.servicebus.windows.net".to_owned()),
            event_hub: Some("orders".to_owned()),
            consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
            auth,
        }),
        given,
        paths: DataPaths::new(root.path().join("data")),
    }
}

fn connection_string_auth() -> AuthMethodConfig {
    AuthMethodConfig::ConnectionString {
        stored: StoredSecret::LocalDb,
    }
}

fn client_secret_auth() -> AuthMethodConfig {
    AuthMethodConfig::Credential {
        credential: CredentialKindConfig::ClientSecret {
            tenant_id: "tenant".to_owned(),
            client_id: "client".to_owned(),
        },
    }
}

#[tokio::test]
async fn a_typed_connection_string_resolves_without_the_secrets_file() {
    let root = tempfile::tempdir().unwrap();
    let request = request(
        connection_string_auth(),
        SensitiveValues {
            connection_string: Some(SecretString::from(CONNECTION_STRING.to_owned())),
            client_secret: None,
        },
        &root,
    );

    let target = resolve_target(&request).await.unwrap();

    assert!(matches!(target.auth, AuthMethod::ConnectionString { .. }));
    assert!(!request.paths.secrets_db().exists());
}

#[tokio::test]
async fn a_stored_connection_string_resolves_through_the_secrets_file() {
    let root = tempfile::tempdir().unwrap();
    let request = request(connection_string_auth(), SensitiveValues::default(), &root);
    SecretsStore::open(&request.paths)
        .unwrap()
        .put(
            CONNECTION_STRING_SECRET,
            &SecretString::from(CONNECTION_STRING.to_owned()),
        )
        .unwrap();

    let target = resolve_target(&request).await.unwrap();

    assert!(matches!(
        target.auth,
        AuthMethod::ConnectionString { connection_string } if connection_string.expose() == CONNECTION_STRING
    ));
}

#[tokio::test]
async fn a_stored_client_secret_is_read_for_its_client() {
    let root = tempfile::tempdir().unwrap();
    let request = request(client_secret_auth(), SensitiveValues::default(), &root);
    SecretsStore::open(&request.paths)
        .unwrap()
        .put(
            &client_secret_name("client"),
            &SecretString::from("stored-secret".to_owned()),
        )
        .unwrap();

    let target = resolve_target(&request).await.unwrap();

    assert_eq!(target.client_secret.unwrap().expose(), "stored-secret");
}

#[tokio::test]
async fn a_client_secret_neither_typed_nor_stored_fails_naming_the_client() {
    let root = tempfile::tempdir().unwrap();
    let request = request(client_secret_auth(), SensitiveValues::default(), &root);

    let error = resolve_target(&request).await.unwrap_err();

    assert!(
        error
            .to_string()
            .starts_with("No client secret for client 'client'"),
        "{error:#}"
    );
}

fn one_message_options(destination: &std::path::Path) -> ExportOptions {
    crate::read::inspector::export_options(
        shared_eventhub_persistence::export::ExportFormat::Txt,
        true,
        destination,
    )
}

fn unknown_key() -> MessageKey {
    MessageKey {
        partition_id: "0".to_owned(),
        sequence_number: 7,
    }
}

#[tokio::test]
async fn exporting_a_message_the_archive_lacks_fails_and_writes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(root.path().join("data"));
    let slot = StoreSlot::new(paths.clone());
    let destination = root.path().join("one.txt");

    let error = export_one(
        &slot,
        &paths,
        unknown_key(),
        one_message_options(&destination),
        destination.clone(),
    )
    .await
    .unwrap_err();

    assert!(
        format!("{error:#}").contains("No message is stored"),
        "{error:#}"
    );
    assert!(!destination.exists());
}

#[tokio::test]
async fn a_scan_task_that_dies_is_reported_as_a_failed_search() {
    let joined = tokio::task::spawn_blocking(|| -> Option<Vec<u64>> { panic!("scan bug") }).await;

    let result = scan_result(joined);

    assert!(matches!(result, Some(Err(text)) if text.contains("stopped unexpectedly")));
}

#[test]
fn a_scan_whose_search_moved_on_posts_nothing() {
    assert!(scan_result(Ok(None)).is_none());
}

#[tokio::test]
async fn an_export_task_that_dies_is_reported_as_a_failure() {
    let joined =
        tokio::task::spawn_blocking(|| -> anyhow::Result<()> { panic!("export bug") }).await;

    let error = export_result(joined).unwrap_err();

    assert!(
        format!("{error:#}").contains("stopped unexpectedly"),
        "{error:#}"
    );
}

#[tokio::test]
async fn exporting_from_a_previous_data_folder_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let old = DataPaths::new(root.path().join("old"));
    let slot = StoreSlot::new(DataPaths::new(root.path().join("new")));
    let destination = root.path().join("one.txt");

    let error = export_one(
        &slot,
        &old,
        unknown_key(),
        one_message_options(&destination),
        destination.clone(),
    )
    .await
    .unwrap_err();

    assert!(
        format!("{error:#}").contains("data folder changed"),
        "{error:#}"
    );
    assert!(!destination.exists());
}

#[tokio::test]
async fn preparing_a_session_reports_a_store_that_cannot_open() {
    let root = tempfile::tempdir().unwrap();
    let request = request(connection_string_auth(), SensitiveValues::default(), &root);
    std::fs::create_dir_all(request.paths.base()).unwrap();
    std::fs::write(request.paths.base().join("notes.txt"), "not eh's").unwrap();
    let slot = StoreSlot::new(request.paths.clone());

    struct Unused;
    impl UiPort for Unused {
        fn deliver(&self, _delivery: UiDelivery, _ack: oneshot::Sender<()>) {}
        fn done(&self, _done: EngineDone) {}
    }

    let (_start, started) = oneshot::channel();
    let error = prepare_session(
        &Handle::current(),
        1,
        request,
        &slot,
        Arc::new(Unused),
        started,
    )
    .await
    .err()
    .unwrap();

    assert!(
        format!("{error:#}").contains("is not an eh data folder"),
        "{error:#}"
    );
}

/// Sends the session's end to the test; acks every delivery.
struct EndOnly(tokio::sync::mpsc::UnboundedSender<EngineDone>);

impl UiPort for EndOnly {
    fn deliver(&self, _delivery: UiDelivery, ack: oneshot::Sender<()>) {
        ack.send(()).unwrap();
    }

    fn done(&self, done: EngineDone) {
        self.0.send(done).unwrap();
    }
}

#[tokio::test]
async fn a_crafted_connection_string_reaches_no_error_surface() {
    const MARKER: &str = "marker-41077";
    let crafted =
        format!("Endpoint=sb://{MARKER}.example.com/;SharedAccessKey={MARKER};EntityPath=orders");
    let root = tempfile::tempdir().unwrap();
    let mut request = request(
        connection_string_auth(),
        SensitiveValues {
            connection_string: Some(SecretString::from(crafted)),
            client_secret: None,
        },
        &root,
    );
    request.config.target.namespace = None;
    let slot = StoreSlot::new(request.paths.clone());
    let (ends, mut ended) = tokio::sync::mpsc::unbounded_channel();
    let (start, started) = oneshot::channel();

    let session = prepare_session(
        &Handle::current(),
        1,
        request,
        &slot,
        Arc::new(EndOnly(ends)),
        started,
    )
    .await
    .unwrap();
    let mut form = crate::form::ConnectForm::default();
    form.connection_string.edit_mut().replace_text("x");
    let mut state = AppState::new(None, form, None, common_cli::app_logger::FileLogging::Off);
    drop(state.apply(Input::ConnectPressed));
    drop(state.apply(Input::SessionReady {
        attempt: 1,
        result: Ok(session),
    }));
    start.send(()).unwrap();
    let done = ended.recv().await.unwrap();
    drop(state.apply(Input::EngineDone(done)));

    let crate::state::ConnectionState::Failed(failure) = &state.connection else {
        panic!("expected Failed, got {:?}", state.connection);
    };
    assert!(!failure.text.contains(MARKER), "{}", failure.text);
    assert!(
        state
            .toasts
            .iter()
            .all(|toast| !toast.text.contains(MARKER))
    );
}
