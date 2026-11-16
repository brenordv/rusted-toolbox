use super::*;
use crate::state::ConnectionState;
use shared_eventhub_core::connection::EventHubIdentity;
use shared_eventhub_core::engine::{
    Checkpoint, EngineCommand, EngineEvent, MessageSink, RunEnd, StoreOutcome,
};
use shared_eventhub_core::filters::{CaptureFilters, TextFilter};
use shared_eventhub_core::models::CapturedMessage;
use std::sync::Mutex;

/// Records the order of sink calls.
#[derive(Clone, Default)]
struct Recording(Arc<Mutex<Vec<String>>>);

impl Recording {
    fn log(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

impl MessageSink for Recording {
    async fn bind(&self, hub: &EventHubIdentity) -> anyhow::Result<()> {
        self.0
            .lock()
            .unwrap()
            .push(format!("bind {}", hub.event_hub_name));
        Ok(())
    }

    async fn store(
        &self,
        message: &CapturedMessage,
        _checkpoint: &Checkpoint,
        _dedup: bool,
    ) -> anyhow::Result<StoreOutcome> {
        self.0.lock().unwrap().push(format!(
            "store {}",
            message.message_id.as_deref().unwrap_or("")
        ));
        Ok(StoreOutcome { newly_stored: true })
    }

    async fn load_checkpoint(&self, _partition_id: &str) -> anyhow::Result<Option<Checkpoint>> {
        Ok(None)
    }
}

struct NoServices;

impl Services for NoServices {
    fn prepare_session(&self, _attempt: u64, _request: ConnectRequest) {}
    fn save_config(&self, _request: SaveRequest) {}
    fn switch_data_folder(&self, _paths: DataPaths) {}
    fn reopen_window(&self) {}
    fn quit(&self) {}
    fn rebuild_search(&self, _request: RebuildRequest) {}
    fn export_one(&self, _request: ExportOneRequest) {}
    fn search_archive(&self, _request: PageRequest) {}
    fn bulk_export(&self, _request: BulkRequest) {}
    fn start_send(&self, _request: SendRequest) {}
}

fn bulk_request(folder: PathBuf, paths: DataPaths) -> BulkRequest {
    BulkRequest {
        export: 1,
        query: shared_eventhub_persistence::search::SearchQuery::default(),
        options: ExportOptions {
            format: shared_eventhub_persistence::export::ExportFormat::Txt,
            include_metadata: true,
            condense_output: false,
            use_local_time: false,
            export_folder: folder,
            skip_already_exported: false,
        },
        cancel: Arc::new(AtomicBool::new(false)),
        paths,
    }
}

#[test]
fn a_demo_bulk_export_reads_and_writes_only_under_its_root() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let request = bulk_request(
        outside.path().join("picked"),
        DataPaths::new(outside.path().join("data")),
    );

    let rooted = rooted_bulk(root.path(), request);

    assert_eq!(rooted.paths, data_paths(root.path()));
    assert!(rooted.options.export_folder.starts_with(root.path()));
}

#[test]
fn the_seeded_export_scenario_starts_searching_the_archive() {
    let root = tempfile::tempdir().unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let state = initial_state(
        Scenario::SeededExport,
        root.path(),
        Box::new(NoServices),
        rt.handle(),
    );

    assert_eq!(state.section, Section::Export);
    assert!(matches!(
        state.archive.results,
        crate::archive::results::Results::Loading { .. }
    ));
    assert!(Scenario::SeededExport.ticks() && Scenario::SeededExport.seeds_archive());
}

#[test]
fn seeding_fills_the_demo_archive_past_the_export_cap() {
    let root = tempfile::tempdir().unwrap();
    let slot = StoreSlot::new(data_paths(root.path()));
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(seed_archive(&slot, root.path())).unwrap();

    let store = rt.block_on(slot.acquire(&data_paths(root.path()))).unwrap();
    let page = store
        .search(&shared_eventhub_persistence::search::SearchQuery::default())
        .unwrap();
    assert!(page.items.len() > crate::read::rows::ROW_CAP);
}

#[test]
fn every_scenario_name_parses_and_an_unknown_one_lists_them() {
    for (name, scenario) in Scenario::NAMES {
        assert_eq!(Scenario::parse(name), Ok(scenario));
    }

    let error = Scenario::parse("nope").unwrap_err();

    assert!(error.contains("'nope'"), "{error}");
    assert!(error.contains("live, burst, quiet"), "{error}");
}

#[tokio::test(start_paused = true)]
async fn the_hub_binds_first_starts_and_stores_each_message_before_emitting_it() {
    let sink = Recording::default();
    let (mut channels, run) = hub::build(Scenario::Live, CaptureFilters::default(), sink.clone());
    let running = tokio::spawn(run);

    let first = channels.events.recv().await.unwrap();
    let message = loop {
        if let EngineEvent::Message(message) = channels.events.recv().await.unwrap() {
            break message;
        }
    };
    channels.commands.send(EngineCommand::Stop).await.unwrap();
    let summary = running.await.unwrap().unwrap();

    let log = sink.log();
    assert_eq!(log[0], "bind demo");
    assert!(log.contains(&format!("store {}", message.message_id.unwrap())));
    assert!(matches!(first, EngineEvent::Started { partitions } if partitions.len() == 4));
    assert_eq!(summary.end, RunEnd::Stopped);
}

#[tokio::test(start_paused = true)]
async fn new_filters_stop_non_matching_messages_but_not_their_storage() {
    let sink = Recording::default();
    let (mut channels, run) = hub::build(Scenario::Live, CaptureFilters::default(), sink.clone());
    let running = tokio::spawn(run);
    drop(channels.events.recv().await);

    channels
        .commands
        .send(EngineCommand::UpdateFilters(CaptureFilters {
            time: None,
            text: Some(TextFilter::Contains(vec![
                "no demo body has this".to_owned(),
            ])),
        }))
        .await
        .unwrap();
    while channels.events.try_recv().is_ok() {}
    tokio::time::sleep(Duration::from_millis(200)).await;
    let emitted_after = std::iter::from_fn(|| channels.events.try_recv().ok())
        .filter(|event| matches!(event, EngineEvent::Message(_)))
        .count();
    channels.commands.send(EngineCommand::Stop).await.unwrap();
    let summary = running.await.unwrap().unwrap();

    assert_eq!(emitted_after, 0);
    assert!(summary.stats.filtered_out > 0);
}

#[tokio::test(start_paused = true)]
async fn a_full_event_channel_counts_dropped_view_events() {
    let (channels, run) = hub::build(
        Scenario::Burst,
        CaptureFilters::default(),
        Recording::default(),
    );
    let running = tokio::spawn(run);

    tokio::time::sleep(Duration::from_secs(1)).await;
    channels.commands.send(EngineCommand::Stop).await.unwrap();
    let summary = running.await.unwrap().unwrap();

    assert!(summary.stats.ui_dropped > 0);
}

#[tokio::test]
async fn the_start_fail_scenario_reports_a_fatal_error_and_fails() {
    let (mut channels, run) = hub::build(
        Scenario::StartFail,
        CaptureFilters::default(),
        Recording::default(),
    );

    let error = run.await.unwrap_err();

    assert!(format!("{error:#}").contains("refuses"), "{error:#}");
    assert!(matches!(
        channels.events.recv().await,
        Some(EngineEvent::Error { fatal: true, .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn the_partition_fail_scenario_ends_with_a_failed_partition() {
    let (mut channels, run) = hub::build(
        Scenario::PartitionFail,
        CaptureFilters::default(),
        Recording::default(),
    );
    let running = tokio::spawn(run);

    let failed = loop {
        match channels.events.recv().await.unwrap() {
            EngineEvent::PartitionEnded { partition_id, .. } => break partition_id,
            _ => continue,
        }
    };
    channels.commands.send(EngineCommand::Stop).await.unwrap();
    let summary = running.await.unwrap().unwrap();

    assert_eq!(failed, "2");
    assert!(matches!(
        summary.partitions.get("2"),
        Some(shared_eventhub_core::engine::PartitionEnd::Failed(_))
    ));
}

fn seeded(scenario: Scenario, root: &Path, rt: &tokio::runtime::Runtime) -> AppState {
    initial_state(scenario, root, Box::new(NoServices), rt.handle())
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn seeded_scenarios_reach_their_states_through_apply_and_render() {
    let root = tempfile::tempdir().unwrap();
    let rt = runtime();

    let failed = seeded(Scenario::SeededFailed, root.path(), &rt);
    let connect = seeded(Scenario::SeededConnect, root.path(), &rt);
    let empty = seeded(Scenario::SeededEmpty, root.path(), &rt);
    let read = seeded(Scenario::SeededRead, root.path(), &rt);
    let search = seeded(Scenario::SeededSearch, root.path(), &rt);
    let details = seeded(Scenario::ErrorDetails, root.path(), &rt);
    let toasts = seeded(Scenario::ToastStack, root.path(), &rt);
    let toasts_open = seeded(Scenario::ToastStackOpen, root.path(), &rt);
    let partitions = seeded(Scenario::SeededPartitions, root.path(), &rt);

    let shown = partitions.read.shown.as_ref().unwrap();
    assert_eq!(shown.stats.per_partition.len(), 32);
    assert!(
        shown
            .stats
            .per_partition
            .values()
            .all(|partition| partition.received >= 10_000_000)
    );
    assert!(
        partitions
            .read
            .session_log
            .iter()
            .any(|line| crate::state::error_preview(&line.text).cut)
    );
    assert_eq!(toasts.toasts().entries().len(), 3);
    assert!(!toasts.toasts().stack().expanded());
    assert!(toasts_open.toasts().stack().expanded());
    assert!(matches!(failed.connection, ConnectionState::Failed(_)));
    assert!(matches!(details.connection, ConnectionState::Failed(_)));
    assert!(
        details
            .details()
            .is_some_and(|open| open.text.chars().count() > 2_000)
    );
    assert!(connect.sheet.is_some() && connect.show_issues);
    assert!(matches!(empty.connection, ConnectionState::Disconnected));
    assert!(matches!(read.connection, ConnectionState::Connected { .. }));
    assert_eq!(read.read.rows.displayed().len(), 5_000);
    assert!(
        read.read
            .inspector
            .selected
            .as_ref()
            .unwrap()
            .message
            .body
            .starts_with('{')
    );
    assert!(read.read.shown.is_some());
    assert!(search.read.list_len() > 0 && search.read.list_len() < 5_000);
    assert_eq!(search.read.search.rebuilding(), None);
    for state in [
        failed,
        connect,
        empty,
        read,
        search,
        details,
        toasts,
        toasts_open,
        partitions,
    ] {
        let mut rt = rune::Runtime::new(
            state,
            crate::views::view,
            (1280.0, 800.0),
            rune::Env::new(true, 1.0),
        );
        rt.frame(0.0);
        assert!(!rt.elements().is_empty());
    }
}

#[test]
fn the_demo_data_folder_lies_under_its_root() {
    let root = tempfile::tempdir().unwrap();

    let paths = data_paths(root.path());
    let state = seeded(Scenario::SeededEmpty, root.path(), &runtime());

    assert!(paths.base().starts_with(root.path()));
    assert!(paths.inbound_dir(DEMO_INBOUND).starts_with(root.path()));
    assert!(state.form.data_paths().base().starts_with(root.path()));
}

fn send_request(count: u64, pacing: shared_eventhub_core::producer::Pacing) -> SendRequest {
    let form = crate::form::ConnectForm::default();
    let (config, given) = form.to_config();
    SendRequest {
        run: 1,
        target: ConnectRequest {
            config,
            given,
            paths: form.data_paths(),
        },
        event: shared_eventhub_core::producer::OutgoingEvent {
            body: b"ping".to_vec(),
            ..shared_eventhub_core::producer::OutgoingEvent::default()
        },
        template: None,
        plan: shared_eventhub_core::producer::SendPlan::new(
            count,
            shared_eventhub_core::producer::Routing::automatic(),
            pacing,
        )
        .unwrap(),
        cancel: Arc::new(AtomicBool::new(false)),
    }
}

/// What the demo send posted, with the virtual time each arrived at.
#[derive(Clone, Default)]
struct SendPosts(Arc<Mutex<Vec<(Duration, SendInput)>>>);

impl SendPosts {
    fn sink(&self, started: tokio::time::Instant) -> impl Fn(SendInput) + Send + use<> {
        let posts = self.clone();
        move |input| posts.0.lock().unwrap().push((started.elapsed(), input))
    }

    fn progress_times(&self) -> Vec<(u64, Duration)> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(at, input)| match input {
                SendInput::Progress { sent, .. } => Some((*sent, *at)),
                _ => None,
            })
            .collect()
    }

    fn finished(&self) -> Option<(shared_eventhub_core::producer::SendEnd, u64, Duration)> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .find_map(|(at, input)| match input {
                SendInput::Finished { summary, .. } => {
                    Some((summary.end.clone(), summary.sent, *at))
                }
                _ => None,
            })
    }
}

#[tokio::test(start_paused = true)]
async fn a_paced_demo_send_posts_one_event_per_interval_the_first_at_once() {
    let posts = SendPosts::default();
    let started = tokio::time::Instant::now();
    let pacing = shared_eventhub_core::producer::Pacing::interval(Duration::from_secs(1)).unwrap();

    send::demo_send(send_request(3, pacing), posts.sink(started)).await;

    assert_eq!(
        posts.progress_times(),
        vec![
            (1, Duration::ZERO),
            (2, Duration::from_secs(1)),
            (3, Duration::from_secs(2))
        ]
    );
    assert!(matches!(
        posts.finished(),
        Some((shared_eventhub_core::producer::SendEnd::Completed, 3, _))
    ));
}

#[tokio::test(start_paused = true)]
async fn a_cancel_during_a_paced_wait_ends_the_demo_send_within_a_poll() {
    let posts = SendPosts::default();
    let started = tokio::time::Instant::now();
    let pacing = shared_eventhub_core::producer::Pacing::interval(Duration::from_secs(1)).unwrap();
    let request = send_request(3, pacing);
    let cancel = Arc::clone(&request.cancel);
    let running = tokio::spawn(send::demo_send(request, posts.sink(started)));
    tokio::time::sleep(Duration::from_millis(1_500)).await;

    cancel.store(true, std::sync::atomic::Ordering::Release);
    running.await.unwrap();

    let (end, sent, at) = posts.finished().unwrap();
    assert_eq!(end, shared_eventhub_core::producer::SendEnd::Cancelled);
    assert_eq!(sent, 2);
    assert!(
        at <= Duration::from_millis(1_500) + shared_eventhub_core::cancel::CANCEL_POLL,
        "{at:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_batched_demo_send_of_one_event_ends_after_one_step() {
    let posts = SendPosts::default();
    let started = tokio::time::Instant::now();

    send::demo_send(
        send_request(1, shared_eventhub_core::producer::Pacing::batched()),
        posts.sink(started),
    )
    .await;

    assert!(matches!(
        posts.finished(),
        Some((shared_eventhub_core::producer::SendEnd::Completed, 1, _))
    ));
}

#[test]
fn the_send_scenarios_open_the_send_section() {
    let root = tempfile::tempdir().unwrap();
    let rt = runtime();

    let idle = seeded(Scenario::Send, root.path(), &rt);
    let paced = seeded(Scenario::SendPaced, root.path(), &rt);

    assert_eq!(idle.section, Section::Send);
    assert!(matches!(idle.send.run, crate::send::run::SendRun::Idle));
    assert!(matches!(
        paced.send.run,
        crate::send::run::SendRun::Preparing { requested: 10, .. }
    ));
    for state in [idle, paced] {
        let mut rt = rune::Runtime::new(
            state,
            crate::views::view,
            (1280.0, 800.0),
            rune::Env::new(true, 1.0),
        );
        rt.frame(0.0);
        assert!(!rt.elements().is_empty());
    }
}
