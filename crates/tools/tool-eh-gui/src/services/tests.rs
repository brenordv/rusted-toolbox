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

/// A slot whose current folder under `root` holds `count` stored messages ("order 0", ...).
async fn seeded_slot(root: &std::path::Path, count: i64) -> (StoreSlot, DataPaths) {
    use shared_eventhub_core::connection::EventHubIdentity;
    use shared_eventhub_core::engine::{Checkpoint, MessageSink};

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
            format: shared_eventhub_persistence::export::ExportFormat::Txt,
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
