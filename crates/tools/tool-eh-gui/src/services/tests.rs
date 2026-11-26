use super::*;
use crate::form::ConnectForm;
use crate::state::{Effect, Outcome, SheetPurpose};
use common_azure::auth::AuthMethod;
use common_cli::app_logger::FileLogging;
use common_utils::string_utils::SecretString;
use eh_core::connection::DEFAULT_CONSUMER_GROUP;
use eh_persistence::config::{AuthMethodConfig, CredentialKindConfig, StoredSecret, TargetConfig};
use eh_persistence::secrets::{CONNECTION_STRING_SECRET, client_secret_name};

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
        eh_persistence::export::ExportFormat::Txt,
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
async fn a_blocking_task_that_dies_is_reported_with_what_stopped() {
    let joined =
        tokio::task::spawn_blocking(|| -> anyhow::Result<SearchPage> { panic!("search bug") })
            .await;

    let error = blocking_result(joined, "Searching the archive stopped unexpectedly").unwrap_err();

    assert!(
        format!("{error:#}").contains("Searching the archive stopped unexpectedly"),
        "{error:#}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_close_that_never_ends_is_given_up_on_at_the_bound() {
    let folder = DataPaths::new("data");
    let started = tokio::time::Instant::now();

    let finished = closed_within(std::future::pending::<()>(), ARCHIVE_CLOSE_BOUND, &folder).await;

    assert!(!finished);
    assert_eq!(started.elapsed(), ARCHIVE_CLOSE_BOUND);
}

#[tokio::test(start_paused = true)]
async fn a_close_that_ends_is_not_waited_for_any_longer() {
    let folder = DataPaths::new("data");
    let started = tokio::time::Instant::now();

    let finished = closed_within(std::future::ready(()), ARCHIVE_CLOSE_BOUND, &folder).await;

    assert!(finished);
    assert_eq!(started.elapsed(), Duration::ZERO);
}

#[tokio::test]
async fn a_slot_closed_for_quit_within_the_bound_refuses_the_next_open() {
    let root = tempfile::tempdir().unwrap();
    let (slot, paths) = seeded_slot(root.path(), 1).await;

    let finished = closed_within(slot.close_for_quit(), ARCHIVE_CLOSE_BOUND, &paths).await;

    assert!(finished);
    assert!(slot.acquire(&paths).await.is_err());
}

/// A slot whose current folder under `root` holds `count` stored messages ("order 0", ...).
async fn seeded_slot(root: &std::path::Path, count: i64) -> (StoreSlot, DataPaths) {
    use eh_core::connection::EventHubIdentity;
    use eh_core::engine::{Checkpoint, MessageSink};

    let paths = DataPaths::new(root.join("data"));
    let slot = StoreSlot::new(paths.clone());
    let store = slot.acquire(&paths).await.unwrap();
    store
        .bind(&EventHubIdentity {
            fully_qualified_namespace: "orders.servicebus.windows.net".to_owned(),
            event_hub_name: "orders".to_owned(),
            created_on: None,
        })
        .await
        .unwrap();
    for sequence in 0..count {
        let message =
            crate::read::test_support::captured("0", sequence, &format!("order {sequence}"));
        let checkpoint = Checkpoint {
            partition_id: "0".to_owned(),
            sequence_number: sequence,
            offset: None,
            enqueued_time: message.enqueued_time,
            updated_at: message.processed_at,
        };
        store.store(&message, &checkpoint, true).await.unwrap();
    }
    (slot, paths)
}

fn bulk_request(paths: DataPaths, folder: std::path::PathBuf) -> BulkRequest {
    BulkRequest {
        export: 1,
        query: SearchQuery::default(),
        options: ExportOptions {
            format: eh_persistence::export::ExportFormat::Txt,
            include_metadata: false,
            condense_output: false,
            use_local_time: false,
            export_folder: folder,
            skip_already_exported: false,
        },
        cancel: Arc::new(AtomicBool::new(false)),
        paths,
    }
}

#[tokio::test]
async fn a_page_search_reads_the_current_folders_archive() {
    let root = tempfile::tempdir().unwrap();
    let (slot, paths) = seeded_slot(root.path(), 1).await;

    let page = search_page(&slot, &paths, SearchQuery::default())
        .await
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].body, "order 0");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bulk_export_writes_every_match_and_reports_progress() {
    let root = tempfile::tempdir().unwrap();
    let (slot, paths) = seeded_slot(root.path(), 3).await;
    let folder = root.path().join("exports");
    let posted = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = Arc::clone(&posted);

    let outcome = bulk_export(&slot, bulk_request(paths, folder.clone()), move |counts| {
        sink.lock().unwrap().push(counts);
    })
    .await
    .unwrap();

    assert_eq!(outcome.exported, 3);
    assert!(!posted.lock().unwrap().is_empty());
    assert!(folder.exists());
}

#[tokio::test]
async fn exporting_matches_from_a_previous_data_folder_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let old = DataPaths::new(root.path().join("old"));
    let slot = StoreSlot::new(DataPaths::new(root.path().join("new")));
    let folder = root.path().join("exports");

    let error = bulk_export(&slot, bulk_request(old, folder.clone()), |_| {})
        .await
        .unwrap_err();

    assert!(
        format!("{error:#}").contains("data folder changed"),
        "{error:#}"
    );
    assert!(!folder.exists());
}

#[tokio::test]
async fn searching_a_previous_data_folder_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let old = DataPaths::new(root.path().join("old"));
    let slot = StoreSlot::new(DataPaths::new(root.path().join("new")));

    let error = search_page(&slot, &old, SearchQuery::default())
        .await
        .unwrap_err();

    assert!(
        format!("{error:#}").contains("data folder changed"),
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
            .toasts()
            .entries()
            .iter()
            .all(|toast| !toast.options.text.contains(MARKER))
    );
}

#[test]
fn two_raw_opens_of_one_secrets_file_collide() {
    let root = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(root.path().join("data"));
    let first = SecretsStore::open(&paths).unwrap();

    let second = SecretsStore::open(&paths);

    assert!(second.is_err());
    drop(first);
}

#[test]
fn overlapping_gated_opens_both_succeed() {
    let root = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(root.path().join("data"));
    let other = paths.clone();

    let results = std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            open_secrets(&paths, |_| {
                std::thread::sleep(Duration::from_millis(100));
                Ok(())
            })
        });
        let second = scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(20));
            open_secrets(&other, |_| Ok(()))
        });
        (first.join().unwrap(), second.join().unwrap())
    });

    assert!(results.0.is_ok() && results.1.is_ok(), "{results:?}");
}

#[test]
fn a_panic_inside_a_gated_open_does_not_lock_the_next_one_out() {
    let root = tempfile::tempdir().unwrap();
    let paths = DataPaths::new(root.path().join("data"));
    let panicked = std::thread::scope(|scope| {
        scope
            .spawn(|| open_secrets(&paths, |_| -> anyhow::Result<()> { panic!("inside") }))
            .join()
            .is_err()
    });

    let next = open_secrets(&paths, |_| Ok(()));

    assert!(panicked);
    assert!(next.is_ok(), "{next:?}");
}

/// Writes `text` as `config.json` in `dir`, creating `dir`.
fn write_config(dir: &Path, text: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(eh_persistence::config::CONFIG_FILE_NAME), text).unwrap();
}

const CREDENTIAL_CONFIG: &str = r#"{"tool":"eh","schema_version":1,"target":{"auth":{"method":"credential","credential":{"kind":"developer-tools"}}}}"#;

#[tokio::test]
async fn listing_a_workspace_answers_its_profiles_for_the_request() {
    let root = tempfile::tempdir().unwrap();
    write_config(&root.path().join("orders"), CREDENTIAL_CONFIG);

    let answer = listed_profiles(4, Workspace::at(root.path().to_owned())).await;

    let Input::ProfilesListed {
        request: 4,
        result: Ok(entries),
    } = answer
    else {
        panic!("expected the listing for request 4, got {answer:?}");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].profile.name(), "orders");
}

#[tokio::test]
async fn a_workspace_that_cannot_be_listed_answers_why() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("not-a-folder");
    std::fs::write(&file, "x").unwrap();

    let answer = listed_profiles(2, Workspace::at(file)).await;

    let Input::ProfilesListed {
        request: 2,
        result: Err(text),
    } = answer
    else {
        panic!("expected a failed listing, got {answer:?}");
    };
    assert!(text.starts_with("Failed to list the workspace"), "{text}");
}

#[tokio::test]
async fn loading_a_profile_answers_its_config_for_the_request() {
    let root = tempfile::tempdir().unwrap();
    write_config(root.path(), CREDENTIAL_CONFIG);

    let answer = loaded_profile(3, root.path()).await;

    assert!(
        matches!(
            &answer,
            Input::ProfileLoaded {
                request: 3,
                result: Ok(eh_persistence::config::ConfigLoad::Loaded(_)),
            }
        ),
        "{answer:?}"
    );
}

#[tokio::test]
async fn a_profile_whose_config_is_broken_answers_why_without_its_values() {
    let root = tempfile::tempdir().unwrap();
    write_config(
        root.path(),
        r#"{"tool":"eh","schema_version":1,"target":{"auth":{"method":"connection-string","stored":"SharedAccessKey=marker-4410"}}}"#,
    );

    let answer = loaded_profile(5, root.path()).await;

    let Input::ProfileLoaded {
        request: 5,
        result: Err(text),
    } = answer
    else {
        panic!("expected a failed load, got {answer:?}");
    };
    assert!(
        text.contains("is an eh config, but it is not valid"),
        "{text}"
    );
    assert!(!text.contains("marker-4410"), "{text}");
}

fn new_profile_request(workspace: &Workspace, name: &str, data_folder: &str) -> CreateRequest {
    let mut config = ToolConfig::new(TargetConfig {
        namespace: None,
        event_hub: None,
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        auth: connection_string_auth(),
    });
    config.storage.base_data_folder = data_folder.to_owned();
    CreateRequest {
        request: 6,
        workspace: workspace.clone(),
        name: ProfileName::parse(name).unwrap(),
        config,
        given: SensitiveValues {
            connection_string: Some(SecretString::from(CONNECTION_STRING.to_owned())),
            client_secret: None,
        },
        stored_from: None,
    }
}

fn created(answer: Input) -> Result<Profile, String> {
    match answer {
        Input::ProfileCreated { request: 6, result } => result,
        other => panic!("expected the answer to create 6, got {other:?}"),
    }
}

#[tokio::test]
async fn a_created_profile_holds_its_config_and_typed_string_and_lists_ready() {
    let root = tempfile::tempdir().unwrap();
    let workspace = Workspace::at(root.path().join("workspace"));

    let answer = created_profile(new_profile_request(&workspace, "orders-new", ".eh-data")).await;

    let profile = created(answer).unwrap();
    assert_eq!(profile.dir(), workspace.root().join("orders-new"));
    let paths = DataPaths::new(profile.dir().join(".eh-data"));
    let stored = SecretsStore::open(&paths)
        .unwrap()
        .get(CONNECTION_STRING_SECRET)
        .unwrap();
    assert_eq!(
        stored.as_ref().map(SecretString::expose),
        Some(CONNECTION_STRING)
    );
    let entries = workspace.list().await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].profile.name(), "orders-new");
    assert!(entries[0].state.is_ready());
}

#[tokio::test]
async fn a_second_create_of_the_name_in_another_case_fails_and_writes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let workspace = Workspace::at(root.path().join("workspace"));
    created(created_profile(new_profile_request(&workspace, "orders-new", ".eh-data")).await)
        .unwrap();
    let config = workspace
        .root()
        .join("orders-new")
        .join(eh_persistence::config::CONFIG_FILE_NAME);
    let before = std::fs::read(&config).unwrap();

    let answer = created_profile(new_profile_request(&workspace, "ORDERS-NEW", "captures")).await;

    let text = created(answer).unwrap_err();
    assert!(
        text.starts_with("A profile named orders-new already exists"),
        "{text}"
    );
    assert_eq!(std::fs::read(&config).unwrap(), before);
    assert_eq!(std::fs::read_dir(workspace.root()).unwrap().count(), 1);
    assert!(
        !workspace
            .root()
            .join("orders-new")
            .join("captures")
            .exists()
    );
}

#[tokio::test]
async fn a_create_whose_data_folder_is_not_plain_relative_fails_before_writing() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let workspace = Workspace::at(root.path().join("workspace"));
    let folder = outside.path().join("data");

    let answer = created_profile(new_profile_request(
        &workspace,
        "orders-new",
        folder.to_str().unwrap(),
    ))
    .await;

    let text = created(answer).unwrap_err();
    assert!(text.contains("is not a plain relative folder"), "{text}");
    assert!(!workspace.root().exists());
    assert!(!folder.exists());
}

const CLIENT_SECRET: &str = "client-secret-4471";

/// A source data folder under `root` whose secrets file holds `name` = `value`.
fn source_with(root: &tempfile::TempDir, name: &str, value: &str) -> DataPaths {
    let source = DataPaths::new(root.path().join("source"));
    open_secrets(&source, |secrets| {
        secrets.put(name, &SecretString::from(value.to_owned()))
    })
    .unwrap();
    source
}

/// [`new_profile_request`] for `auth` with nothing typed and the stored secret in `source`.
fn copying_request(
    workspace: &Workspace,
    auth: AuthMethodConfig,
    source: Option<DataPaths>,
) -> CreateRequest {
    let mut request = new_profile_request(workspace, "orders-new", ".eh-data");
    request.config.target = TargetConfig {
        namespace: Some("orders.servicebus.windows.net".to_owned()),
        event_hub: Some("orders".to_owned()),
        consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        auth,
    };
    request.given = SensitiveValues::default();
    request.stored_from = source;
    request
}

/// What the new profile's secrets file holds under `name`.
fn new_profile_secret(workspace: &Workspace, name: &str) -> Option<String> {
    let paths = DataPaths::new(workspace.root().join("orders-new").join(".eh-data"));
    open_secrets(&paths, |secrets| secrets.get(name))
        .unwrap()
        .map(|value| value.expose().to_owned())
}

#[rstest::rstest]
#[case::connection_string(connection_string_auth(), CONNECTION_STRING_SECRET.to_owned(), CONNECTION_STRING)]
#[case::client_secret(client_secret_auth(), client_secret_name("client"), CLIENT_SECRET)]
#[tokio::test]
async fn a_create_from_a_stored_secret_copies_it_and_the_source_keeps_it(
    #[case] auth: AuthMethodConfig,
    #[case] name: String,
    #[case] value: &str,
) {
    let root = tempfile::tempdir().unwrap();
    let workspace = Workspace::at(root.path().join("workspace"));
    let source = source_with(&root, &name, value);

    let answer = created_profile(copying_request(&workspace, auth, Some(source.clone()))).await;

    created(answer).unwrap();
    assert_eq!(
        new_profile_secret(&workspace, &name).as_deref(),
        Some(value)
    );
    let kept = open_secrets(&source, |secrets| secrets.get(&name)).unwrap();
    assert_eq!(kept.as_ref().map(SecretString::expose), Some(value));
}

#[tokio::test]
async fn a_source_with_no_secrets_file_fails_and_creates_none_there() {
    let root = tempfile::tempdir().unwrap();
    let workspace = Workspace::at(root.path().join("workspace"));
    let source = DataPaths::new(root.path().join("source"));

    let answer = created_profile(copying_request(
        &workspace,
        connection_string_auth(),
        Some(source.clone()),
    ))
    .await;

    let text = created(answer).unwrap_err();
    assert!(text.contains("There is no secrets file"), "{text}");
    assert!(!source.secrets_db().exists());
    assert!(!workspace.root().exists());
}

#[tokio::test]
async fn a_stored_secret_missing_from_the_source_fails_naming_the_file_and_makes_no_profile() {
    let root = tempfile::tempdir().unwrap();
    let workspace = Workspace::at(root.path().join("workspace"));
    let source = source_with(&root, "something-else", "x");

    let answer = created_profile(copying_request(
        &workspace,
        connection_string_auth(),
        Some(source.clone()),
    ))
    .await;

    let text = created(answer).unwrap_err();
    assert!(
        text.contains(&source.secrets_db().display().to_string()),
        "{text}"
    );
    assert!(
        text.contains("The connection string is not in the secrets file"),
        "{text}"
    );
    assert!(!workspace.root().join("orders-new").exists());
}

#[tokio::test]
async fn a_create_with_no_source_copies_nothing() {
    let root = tempfile::tempdir().unwrap();
    let workspace = Workspace::at(root.path().join("workspace"));
    source_with(&root, CONNECTION_STRING_SECRET, CONNECTION_STRING);

    let answer = created_profile(copying_request(&workspace, connection_string_auth(), None)).await;

    let text = created(answer).unwrap_err();
    assert!(
        text.contains("There is no connection string to save"),
        "{text}"
    );
    assert_eq!(
        new_profile_secret(&workspace, CONNECTION_STRING_SECRET),
        None
    );
}

#[rstest::rstest]
#[case::client_secret(
    client_secret_auth(),
    SensitiveValues::default(),
    Some("client secret")
)]
#[case::connection_string(
    connection_string_auth(),
    SensitiveValues::default(),
    Some("connection string")
)]
#[case::client_secret_typed(
    client_secret_auth(),
    SensitiveValues { connection_string: None, client_secret: Some(SecretString::from(CLIENT_SECRET.to_owned())) },
    None
)]
fn the_copied_kind_is_the_secret_the_config_uses_and_the_request_lacks(
    #[case] auth: AuthMethodConfig,
    #[case] given: SensitiveValues,
    #[case] expected: Option<&str>,
) {
    let root = tempfile::tempdir().unwrap();
    let request = copying_request(&Workspace::at(root.path().to_owned()), auth, None);

    assert_eq!(copied_kind(&request.config, &given), expected);
}

/// `paths` as the window resolves it: eh-gui's working folder is its launch folder.
fn resolved_in(launch: &Path, paths: &DataPaths) -> DataPaths {
    DataPaths::new(launch.join(paths.base()))
}

/// The one effect of `out` that `pick` takes.
fn only<T>(out: Outcome, pick: impl FnMut(Effect) -> Option<T>) -> T {
    let found: Vec<T> = out.effects.into_iter().filter_map(pick).collect();
    let Ok([one]) = <[T; 1]>::try_from(found) else {
        panic!("expected exactly one such effect");
    };
    one
}

#[tokio::test]
async fn a_launch_folder_in_the_workspace_saved_once_opens_as_a_profile_that_connects() {
    let root = tempfile::tempdir().unwrap();
    let launch = root.path().join("orders-new");
    std::fs::create_dir(&launch).unwrap();
    let mut state = AppState::new(None, ConnectForm::default(), None, FileLogging::Off);
    state.set_launch(launch.clone(), Ok(Workspace::at(root.path().to_owned())));
    state
        .form
        .connection_string
        .edit_mut()
        .replace_text(CONNECTION_STRING);
    let save = only(state.apply(Input::SavePressed), |effect| match effect {
        Effect::SaveConfig(save) => Some(save),
        _ => None,
    });
    open_secrets(&resolved_in(&launch, &save.paths), |secrets| {
        save_config(&save.dir, &save.config, secrets, &save.given)
    })
    .unwrap();
    state.apply(Input::ConfigSaved(Ok(())));
    let (request, workspace) = only(
        state.apply(Input::OpenSheet(SheetPurpose::Connect)),
        |effect| match effect {
            Effect::ListProfiles { request, workspace } => Some((request, workspace)),
            _ => None,
        },
    );
    state.apply(listed_profiles(request, workspace).await);
    let home = state.home_for_label("orders-new").unwrap();
    let (request, dir) = only(
        state.apply(Input::ProfilePicked(home)),
        |effect| match effect {
            Effect::LoadProfile { request, dir } => Some((request, dir)),
            _ => None,
        },
    );
    state.apply(loaded_profile(request, &dir).await);
    let connect = only(state.apply(Input::ConnectPressed), |effect| match effect {
        Effect::PrepareSession { request, .. } => Some(request),
        _ => None,
    });

    let target = resolve_target(&connect).await.unwrap();

    assert_eq!(state.home().profile_name(), Some("orders-new"));
    assert!(matches!(
        target.auth,
        AuthMethod::ConnectionString { connection_string } if connection_string.expose() == CONNECTION_STRING
    ));
}
