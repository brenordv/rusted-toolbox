//! What the UI asks of the outside world, and the production implementation over the tokio
//! runtime, the archive slot, and Rune's `AppHandle`.

pub mod sending;

use crate::app::{apply_motion, run_effects};
use crate::appearance::SystemAppearance;
use crate::archive::bulk::{BulkRequest, progress_due};
use crate::archive::{ArchiveInput, PageRequest};
use crate::bridge::{
    EngineDone, EngineSession, FilterHook, SessionSink, UiDelivery, UiPort, spawn_session,
};
use crate::read::inspector::InspectorInput;
use crate::read::search::{REBUILD_DEBOUNCE, RebuildRequest, Rebuilt};
use crate::read::{ExportOneRequest, ReadInput};
use crate::send::SendRequest;
use crate::state::{AppState, Input};
use crate::tasks::StoreSlot;
use crate::views::move_profile_focus;
use crate::views::shell::{keep_toast_focus, return_toast_focus};
use anyhow::Context;
use rune::{AppHandle, Tx};
use shared_eventhub_core::connection::EventHubTarget;
use shared_eventhub_core::engine::CaptureEngine;
use shared_eventhub_core::models::MessageKey;
use shared_eventhub_persistence::config::{
    AuthMethodConfig, SensitiveValues, ToolConfig, load_config, needs_stored_secret,
    resolve_target_with, save_config, with_stored_secrets,
};
use shared_eventhub_persistence::export::{ExportOptions, ExportOutcome, WritingSink};
use shared_eventhub_persistence::paths::{DataPaths, plain_relative};
use shared_eventhub_persistence::search::{SearchPage, SearchQuery};
use shared_eventhub_persistence::secrets::SecretsStore;
use shared_eventhub_persistence::workspace::{ConfigHome, Profile, ProfileName, Workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use tokio::task::JoinError;
use tracing::{debug, error, info, info_span, trace};

/// A request to start a session: the config, the typed secrets, and the data folder it is for.
#[derive(Debug)]
pub struct ConnectRequest {
    pub config: ToolConfig,
    pub given: SensitiveValues,
    pub paths: DataPaths,
}

/// A request to save `config.json`: the config, the typed secrets, the folder `config.json`
/// goes to, and the data folder whose secrets file receives the secrets.
#[derive(Debug)]
pub struct SaveRequest {
    pub config: ToolConfig,
    pub given: SensitiveValues,
    pub dir: PathBuf,
    pub paths: DataPaths,
}

/// A request to save a form as a new profile: the workspace, the checked name, and the config
/// and typed secrets to write there; `request` ties its answer to it.
#[derive(Debug)]
pub struct CreateRequest {
    pub request: u64,
    pub workspace: Workspace,
    pub name: ProfileName,
    pub config: ToolConfig,
    pub given: SensitiveValues,
    /// The data folder whose secrets file holds the stored secret the config uses and `given`
    /// lacks; the new profile gets a copy of it. `None` when every secret the config uses is
    /// in `given`.
    pub stored_from: Option<DataPaths>,
}

/// The side effects the state machine asks for. Each runs in the background and reports back
/// as an [`Input`].
pub trait Services {
    /// Opens the store, resolves the target, builds the sink, and starts the session; reports
    /// [`Input::SessionReady`].
    fn prepare_session(&self, attempt: u64, request: ConnectRequest);
    /// Saves `config.json` and its secrets; reports [`Input::ConfigSaved`].
    fn save_config(&self, request: SaveRequest);
    /// Points the store at `paths` and releases the previous folder's archive.
    fn switch_data_folder(&self, paths: DataPaths);
    /// Lists `workspace`'s profiles; reports [`Input::ProfilesListed`] for `request`.
    fn list_profiles(&self, request: u64, workspace: Workspace);
    /// Reads `dir`'s `config.json`; reports [`Input::ProfileLoaded`] for `request`.
    fn load_profile(&self, request: u64, dir: PathBuf);
    /// Saves the request's config and secrets into a new profile folder; reports
    /// [`Input::ProfileCreated`] for its request.
    fn create_profile(&self, request: CreateRequest);
    fn reopen_window(&self);
    /// Closes the store, then ends the app.
    fn quit(&self);
    /// Runs a quick-search rebuild off the UI thread; reports [`ReadInput::SearchRebuilt`].
    fn rebuild_search(&self, request: RebuildRequest);
    /// Writes one archived message to a file; reports [`InspectorInput::ExportDone`] to the
    /// request's section.
    fn export_one(&self, request: ExportOneRequest);
    /// Reads one page of an archive search; reports [`ArchiveInput::PageLoaded`].
    fn search_archive(&self, request: PageRequest);
    /// Exports every match of a search; reports [`ArchiveInput::BulkProgress`] while it runs
    /// and [`ArchiveInput::BulkDone`] at the end.
    fn bulk_export(&self, request: BulkRequest);
    /// Sends a run's events; reports [`crate::send::SendInput::Started`] once the producer is
    /// open, [`crate::send::SendInput::Progress`] while it sends, and at least one
    /// [`crate::send::SendInput::Finished`].
    fn start_send(&self, request: SendRequest);
}

/// Every opening of a data folder's secrets file goes through this gate, so two openings in
/// this process never collide on the file's lock. The guarded value is `()`, so a panic inside
/// one opening does not lock the next ones out.
static SECRETS_GATE: Mutex<()> = Mutex::new(());

/// Opens the secrets file of `paths` and hands it to `use_store`, holding the gate until the
/// file is closed again. Blocking: call it from a blocking task.
///
/// # Errors
/// Fails when the secrets file cannot be opened, or with `use_store`'s error.
pub fn open_secrets<T>(
    paths: &DataPaths,
    use_store: impl FnOnce(&SecretsStore) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let gate = SECRETS_GATE.lock().unwrap_or_else(PoisonError::into_inner);
    let store = SecretsStore::open(paths)?;
    let result = use_store(&store);
    drop(store);
    drop(gate);
    result
}

/// The production services.
pub struct ProdServices {
    handle: AppHandle<AppState>,
    rt: Handle,
    slot: Arc<StoreSlot>,
}

impl ProdServices {
    /// Builds the services and starts the once-a-second UI tick, which sets `window_failed`
    /// when the window could not be created.
    pub fn new(
        handle: AppHandle<AppState>,
        rt: Handle,
        slot: Arc<StoreSlot>,
        window_failed: Arc<AtomicBool>,
    ) -> Self {
        spawn_tick(&rt, handle.clone(), window_failed);
        spawn_list_flush(&rt, handle.clone());
        Self { handle, rt, slot }
    }
}

/// The answer to listing `workspace` for `request`: its profiles, or why it can't be listed.
pub(crate) async fn listed_profiles(request: u64, workspace: Workspace) -> Input {
    let result = workspace.list().await.map_err(|error| format!("{error:#}"));
    Input::ProfilesListed { request, result }
}

/// The answer to loading `dir`'s `config.json` for `request`: what it holds, or why it can't
/// be read (the loader's text never quotes a value from the file).
pub(crate) async fn loaded_profile(request: u64, dir: &Path) -> Input {
    let result = load_config(dir).await.map_err(|error| format!("{error:#}"));
    Input::ProfileLoaded { request, result }
}

/// The answer to `request`: the new profile, its `config.json` and secrets written, or why it
/// was not saved.
pub async fn created_profile(request: CreateRequest) -> Input {
    let id = request.request;
    let result = create_profile(request)
        .await
        .map_err(|error| format!("{error:#}"));
    Input::ProfileCreated {
        request: id,
        result,
    }
}

/// Reads the stored secret from `stored_from` when one is named, makes the profile folder, then
/// saves the config into it with its secrets in the data folder it names. A data folder that is
/// not plain relative would sit outside the profile folder, so it is refused before anything is
/// written.
async fn create_profile(request: CreateRequest) -> anyhow::Result<Profile> {
    let CreateRequest {
        request,
        workspace,
        name,
        config,
        given,
        stored_from,
    } = request;
    let folder = config.storage.base_data_folder.trim().to_owned();
    if !plain_relative(&folder) {
        anyhow::bail!(
            "A new profile keeps its data folder inside the profile folder, and '{folder}' is not \
             a plain relative folder"
        );
    }
    let (config, given, copied) = match &stored_from {
        Some(source) => {
            let copied = copied_kind(&config, &given);
            let (config, merged) = with_copied_secret(source.clone(), config, given).await?;
            (config, merged, copied)
        }
        None => (config, given, None),
    };
    let profile = workspace.new_profile(&name).await?;
    let paths = ConfigHome::Profile(profile.clone()).data_paths(&folder);
    let dir = profile.dir().to_path_buf();
    let joined = tokio::task::spawn_blocking(move || {
        open_secrets(&paths, |secrets| {
            save_config(&dir, &config, secrets, &given)
        })
    })
    .await;
    blocking_result(joined, "Saving the profile stopped unexpectedly")?;
    if let (Some(kind), Some(source)) = (copied, &stored_from) {
        info!(
            kind,
            from = %source.base().display(),
            profile = profile.name(),
            request,
            "copied a stored secret into the new profile"
        );
    }
    Ok(profile)
}

/// `config` and `given` with the stored secret the config uses read from `source`'s secrets
/// file, on a blocking task. A missing secrets file is refused rather than opened, since opening
/// it would create one there.
async fn with_copied_secret(
    source: DataPaths,
    config: ToolConfig,
    given: SensitiveValues,
) -> anyhow::Result<(ToolConfig, SensitiveValues)> {
    let joined = tokio::task::spawn_blocking(move || {
        let file = source.secrets_db();
        if !file.exists() {
            anyhow::bail!(
                "There is no secrets file at '{}' to copy from",
                file.display()
            );
        }
        let merged = open_secrets(&source, |secrets| {
            with_stored_secrets(&config, given, secrets)
        })
        .with_context(|| format!("Failed to copy the stored secret from '{}'", file.display()))?;
        Ok((config, merged))
    })
    .await;
    blocking_result(joined, "Copying the stored secret stopped unexpectedly")
}

/// The secret `config` uses that `given` lacks, so a copy from the secrets file supplies it, as
/// a log label: the connection string under connection-string auth, else the client secret of a
/// client-secret credential.
fn copied_kind(config: &ToolConfig, given: &SensitiveValues) -> Option<&'static str> {
    let string_missing = match config.target.auth {
        AuthMethodConfig::ConnectionString { .. } => given.connection_string.is_none(),
        AuthMethodConfig::Credential { .. } | AuthMethodConfig::KeyVaultConnectionString { .. } => {
            false
        }
    };
    if string_missing {
        Some("connection string")
    } else if config.client_secret_id().is_some() && given.client_secret.is_none() {
        Some("client secret")
    } else {
        None
    }
}

impl Services for ProdServices {
    fn prepare_session(&self, attempt: u64, request: ConnectRequest) {
        let (handle, rt, slot) = (self.handle.clone(), self.rt.clone(), Arc::clone(&self.slot));
        self.rt.spawn(async move {
            let port: Arc<dyn UiPort> = Arc::new(AppPort(handle.clone()));
            let (start, started) = oneshot::channel();
            let result = prepare_session(&rt, attempt, request, &slot, port, started)
                .await
                .map_err(|error| format!("{error:#}"));
            report_ready(&handle, attempt, result, start);
        });
    }

    fn save_config(&self, request: SaveRequest) {
        let handle = self.handle.clone();
        self.rt.spawn(async move {
            let saved = tokio::task::spawn_blocking(move || {
                open_secrets(&request.paths, |secrets| {
                    save_config(&request.dir, &request.config, secrets, &request.given)
                })
            })
            .await
            .context("Saving the config stopped unexpectedly")
            .and_then(|saved| saved)
            .map_err(|error| format!("{error:#}"));
            apply_from_runtime(&handle, Input::ConfigSaved(saved));
        });
    }

    fn switch_data_folder(&self, paths: DataPaths) {
        self.slot.set_folder(paths);
        let slot = Arc::clone(&self.slot);
        self.rt.spawn(async move { slot.release_stale().await });
    }

    fn list_profiles(&self, request: u64, workspace: Workspace) {
        let handle = self.handle.clone();
        self.rt.spawn(async move {
            let listed = listed_profiles(request, workspace).await;
            apply_from_runtime(&handle, listed);
        });
    }

    fn load_profile(&self, request: u64, dir: PathBuf) {
        let handle = self.handle.clone();
        self.rt.spawn(async move {
            let loaded = loaded_profile(request, &dir).await;
            apply_from_runtime(&handle, loaded);
        });
    }

    fn create_profile(&self, request: CreateRequest) {
        let handle = self.handle.clone();
        self.rt.spawn(async move {
            let created = created_profile(request).await;
            apply_from_runtime(&handle, created);
        });
    }

    fn reopen_window(&self) {
        self.handle.open();
    }

    fn quit(&self) {
        let (handle, slot) = (self.handle.clone(), Arc::clone(&self.slot));
        self.rt.spawn(async move {
            slot.close_for_quit().await;
            handle.quit();
        });
    }

    fn rebuild_search(&self, request: RebuildRequest) {
        spawn_rebuild(&self.rt, self.handle.clone(), request);
    }

    fn export_one(&self, request: ExportOneRequest) {
        spawn_export_one(
            &self.rt,
            self.handle.clone(),
            Arc::clone(&self.slot),
            request,
        );
    }

    fn search_archive(&self, request: PageRequest) {
        spawn_search_archive(
            &self.rt,
            self.handle.clone(),
            Arc::clone(&self.slot),
            request,
        );
    }

    fn bulk_export(&self, request: BulkRequest) {
        spawn_bulk_export(
            &self.rt,
            self.handle.clone(),
            Arc::clone(&self.slot),
            request,
        );
    }

    fn start_send(&self, request: SendRequest) {
        sending::spawn_send(&self.rt, self.handle.clone(), request);
    }
}

/// Reads `request`'s page from the archive of its data folder and posts it.
pub(crate) fn spawn_search_archive(
    rt: &Handle,
    handle: AppHandle<AppState>,
    slot: Arc<StoreSlot>,
    request: PageRequest,
) {
    rt.spawn(async move {
        let started = Instant::now();
        let PageRequest {
            generation,
            query,
            paths,
        } = request;
        let result = search_page(&slot, &paths, query)
            .await
            .map_err(|error| format!("{error:#}"));
        apply_from_runtime(
            &handle,
            Input::Archive(ArchiveInput::PageLoaded {
                generation,
                elapsed_ms: started.elapsed().as_millis(),
                result,
            }),
        );
    });
}

async fn search_page(
    slot: &StoreSlot,
    paths: &DataPaths,
    query: SearchQuery,
) -> anyhow::Result<SearchPage> {
    let store = slot.acquire(paths).await?;
    let joined = tokio::task::spawn_blocking(move || store.search(&query)).await;
    blocking_result(joined, "Searching the archive stopped unexpectedly")
}

/// Exports every match of `request`'s query, posting throttled progress and then the end.
pub(crate) fn spawn_bulk_export(
    rt: &Handle,
    handle: AppHandle<AppState>,
    slot: Arc<StoreSlot>,
    request: BulkRequest,
) {
    rt.spawn(async move {
        let started = Instant::now();
        let export = request.export;
        let progress = handle.clone();
        let post = move |counts: ExportOutcome| {
            apply_from_runtime(
                &progress,
                Input::Archive(ArchiveInput::BulkProgress { export, counts }),
            );
        };
        let result = bulk_export(&slot, request, post)
            .await
            .map_err(|error| format!("{error:#}"));
        apply_from_runtime(
            &handle,
            Input::Archive(ArchiveInput::BulkDone {
                export,
                elapsed_ms: started.elapsed().as_millis(),
                result,
            }),
        );
    });
}

/// Exports every match of `request`'s query from the archive of its data folder, handing
/// `post` the counts at most once per [`PROGRESS_EVERY`](crate::archive::bulk::PROGRESS_EVERY).
pub(crate) async fn bulk_export(
    slot: &StoreSlot,
    request: BulkRequest,
    post: impl Fn(ExportOutcome) + Send + 'static,
) -> anyhow::Result<ExportOutcome> {
    let BulkRequest {
        export,
        query,
        options,
        cancel,
        paths,
    } = request;
    let store = slot.acquire(&paths).await?;
    let joined = tokio::task::spawn_blocking(move || {
        let _span = info_span!("bulk_export", export).entered();
        let mut last = None;
        let report = |outcome: &ExportOutcome| {
            let now = Instant::now();
            if progress_due(last, now) {
                last = Some(now);
                trace!(export, exported = outcome.exported, "bulk export progress");
                post(outcome.clone());
            }
        };
        store.export(&query, &options, report, &cancel)
    })
    .await;
    blocking_result(joined, "The export stopped unexpectedly")
}

/// A blocking task's result, with a task that died reported as `what`.
pub(crate) fn blocking_result<T>(
    joined: Result<anyhow::Result<T>, JoinError>,
    what: &'static str,
) -> anyhow::Result<T> {
    joined.context(what)?
}

/// Runs `request` after the typing pause (follow-ups don't wait), on a blocking thread, and
/// posts the result. A scan whose search moved on posts nothing.
pub(crate) fn spawn_rebuild(rt: &Handle, handle: AppHandle<AppState>, request: RebuildRequest) {
    rt.spawn(async move {
        if !request.follow_up {
            tokio::time::sleep(REBUILD_DEBOUNCE).await;
        }
        if !request.is_current() {
            return;
        }
        let (generation, matcher, upto, follow_up) = (
            request.current,
            request.matcher.clone(),
            request.upto,
            request.follow_up,
        );
        let started = Instant::now();
        let scanned = request.rows.len();
        let joined = tokio::task::spawn_blocking(move || request.scan()).await;
        let Some(result) = scan_result(joined) else {
            return;
        };
        let rebuilt = Rebuilt {
            generation,
            matcher,
            result,
            upto,
            follow_up,
            scanned,
            elapsed_ms: started.elapsed().as_millis(),
        };
        apply_from_runtime(&handle, Input::Read(ReadInput::SearchRebuilt(rebuilt)));
    });
}

/// A scan's outcome: its matches, `None` when its search moved on, or why the scan task died.
pub(crate) fn scan_result(
    joined: Result<Option<Vec<u64>>, JoinError>,
) -> Option<Result<Vec<u64>, String>> {
    match joined {
        Ok(Some(matches)) => Some(Ok(matches)),
        Ok(None) => None,
        Err(error) => Some(Err(format!("the search stopped unexpectedly: {error}"))),
    }
}

/// Writes `request`'s message from the archive of its data folder and posts the result.
pub(crate) fn spawn_export_one(
    rt: &Handle,
    handle: AppHandle<AppState>,
    slot: Arc<StoreSlot>,
    request: ExportOneRequest,
) {
    rt.spawn(async move {
        let ExportOneRequest {
            section,
            owner,
            key,
            options,
            destination,
            paths,
            ..
        } = request;
        let result = export_one(&slot, &paths, key.clone(), options, destination)
            .await
            .map_err(|error| format!("{error:#}"));
        apply_from_runtime(
            &handle,
            Input::inspector(
                section,
                InspectorInput::ExportDone {
                    owner,
                    key,
                    result,
                    now: chrono::Utc::now(),
                },
            ),
        );
    });
}

async fn export_one(
    slot: &StoreSlot,
    paths: &DataPaths,
    key: MessageKey,
    options: ExportOptions,
    destination: PathBuf,
) -> anyhow::Result<()> {
    let store = slot.acquire(paths).await?;
    let joined =
        tokio::task::spawn_blocking(move || store.export_one(&key, &options, &destination)).await;
    export_result(joined)
}

/// A blocking export's outcome, with a task that died reported as a failure.
pub(crate) fn export_result(joined: Result<anyhow::Result<()>, JoinError>) -> anyhow::Result<()> {
    blocking_result(joined, "Exporting the message stopped unexpectedly")
}

/// Applies `input` on the UI thread, runs its effects, and moves its commit as it asks.
pub(crate) fn apply_from_runtime(handle: &AppHandle<AppState>, input: Input) {
    handle.update_if(move |state, tx| apply_input(state, tx, input));
}

/// Applies `input`, moves its commit as it asks, and runs its effects; returns whether anything
/// the view reads changed. Focus on a toast control the input folded away or removed moves to
/// the front card, and focus the Save as profile row asked for moves there.
pub(crate) fn apply_input(state: &mut AppState, tx: &mut Tx, input: Input) -> bool {
    let opener = state.toast_hold();
    let outcome = state.apply(input);
    apply_motion(&outcome, tx);
    run_effects(state, outcome.effects);
    return_toast_focus(state, tx, opener);
    keep_toast_focus(state, tx);
    move_profile_focus(state, tx);
    outcome.changed
}

/// How often arrived rows move onto the list.
const LIST_FLUSH_EVERY: Duration = Duration::from_millis(250);
/// The flush cadence starts this far after the tick's, so a flush rarely shares a frame with
/// the tick's digit roll.
const LIST_FLUSH_OFFSET: Duration = Duration::from_millis(125);

/// Posts [`ReadInput::ListFlush`] four times a second; a flush with nothing arrived redraws
/// nothing.
pub(crate) fn spawn_list_flush(rt: &Handle, handle: AppHandle<AppState>) {
    rt.spawn(async move {
        let first = tokio::time::Instant::now() + LIST_FLUSH_OFFSET;
        let mut cadence = tokio::time::interval_at(first, LIST_FLUSH_EVERY);
        cadence.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            cadence.tick().await;
            apply_from_runtime(&handle, Input::Read(ReadInput::ListFlush));
        }
    });
}

/// Posts a tick once a second; ticks that change nothing do not redraw. Each tick also reads
/// the system appearance (Windows) and posts it when it changed. From the second tick until
/// the window has been seen on screen, it checks for a window failure: Rune keeps running
/// windowless when it cannot create the window (no GPU adapter or device), so that failure is
/// caught here, logged, flagged in `window_failed`, and ends the app.
pub(crate) fn spawn_tick(rt: &Handle, handle: AppHandle<AppState>, window_failed: Arc<AtomicBool>) {
    rt.spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut appearance = SystemAppearance::default();
        let mut ticks: u64 = 0;
        let mut shown = false;
        loop {
            ticker.tick().await;
            ticks += 1;
            shown = shown || handle.is_visible();
            if ticks >= 2
                && !shown
                && let Some(reason) = handle.window_error()
            {
                error!(reason = %reason, "the window could not be created; {GPU_HINT}");
                window_failed.store(true, Ordering::Release);
                handle.quit();
                return;
            }
            if let Some(dark) = appearance.poll() {
                apply_from_runtime(&handle, Input::AppearanceChanged(dark));
            }
            apply_from_runtime(&handle, Input::Tick(chrono::Utc::now()));
        }
    });
}

/// What a window failure means for the user, on every platform.
pub const GPU_HINT: &str = "eh-gui draws with the GPU (DirectX 12 on Windows, Vulkan on Linux, \
                            Metal on macOS); on a machine without one, use the eh CLI";

/// Delivers a session's output to the app through its handle.
pub(crate) struct AppPort(pub(crate) AppHandle<AppState>);

impl UiPort for AppPort {
    fn deliver(&self, delivery: UiDelivery, ack: oneshot::Sender<()>) {
        self.0.update_if(move |state, tx| {
            let outcome = state.apply(Input::Delivery(delivery));
            if ack.send(()).is_err() {
                debug!("the forwarder ended before its delivery was applied");
            }
            apply_motion(&outcome, tx);
            run_effects(state, outcome.effects);
            outcome.changed
        });
    }

    fn done(&self, done: EngineDone) {
        apply_from_runtime(&self.0, Input::EngineDone(done));
    }
}

/// Opens the store, resolves the target, and starts a session whose output goes to `port`.
/// Always runs to completion in its own task (`StoreSlot::acquire_for_session` is not
/// cancel-safe).
pub async fn prepare_session(
    rt: &Handle,
    attempt: u64,
    request: ConnectRequest,
    slot: &StoreSlot,
    port: Arc<dyn UiPort>,
    start: oneshot::Receiver<()>,
) -> anyhow::Result<EngineSession> {
    let store = slot.acquire_for_session(&request.paths).await?;
    let target = resolve_target(&request).await?;
    let sink = WritingSink::new(
        store,
        request.config.disk_write_config(&request.paths),
        request.config.capture.filters.clone(),
    );
    let (channels, run) = CaptureEngine::build(request.config.engine_config(target), sink.clone());
    Ok(spawn_session(
        rt,
        attempt,
        gate(start),
        channels,
        run,
        SessionSink {
            set_filters: filter_hook(&sink),
            finish: async move { sink.finish(|_| {}).await },
        },
        port,
    ))
}

/// A hook that points `sink`'s message-file filters at new capture filters.
pub(crate) fn filter_hook(sink: &WritingSink) -> FilterHook {
    let sink = sink.clone();
    Arc::new(move |filters| sink.set_filters(filters))
}

/// Resolves once `start` fires. A start signal that never comes (its owner panicked) still lets
/// the session run, so the engine never waits forever while holding the archive.
pub(crate) async fn gate(start: oneshot::Receiver<()>) {
    if start.await.is_err() {
        debug!("a session started without its start signal");
    }
}

/// Queues `SessionReady` for `result` and then lets the session start, so the UI holds the
/// session before its first delivery or its end can arrive (Rune applies updates in call order).
pub(crate) fn report_ready(
    handle: &AppHandle<AppState>,
    attempt: u64,
    result: Result<EngineSession, String>,
    start: oneshot::Sender<()>,
) {
    apply_from_runtime(handle, Input::SessionReady { attempt, result });
    if start.send(()).is_err() {
        debug!(attempt, "no session waited for its start signal");
    }
}

/// Builds the target the way `eh read` does: the secrets file is opened (and locked) only when
/// a secret the auth method uses was not typed, and released right after.
///
/// # Errors
/// Fails when a needed secret is neither typed nor stored, or the secrets file cannot be read.
pub async fn resolve_target(request: &ConnectRequest) -> anyhow::Result<EventHubTarget> {
    let config = &request.config;
    if !needs_stored_secret(config, &request.given) {
        return resolve_target_with(config, request.given.clone(), None);
    }

    let (owned, given, paths) = (config.clone(), request.given.clone(), request.paths.clone());
    let target = tokio::task::spawn_blocking(move || {
        open_secrets(&paths, |secrets| {
            resolve_target_with(&owned, given, Some(secrets))
        })
    })
    .await
    .context("Reading the secrets file stopped unexpectedly")??;

    if let Some(client_id) = config.missing_client_secret(&target) {
        anyhow::bail!(
            "No client secret for client '{client_id}': type it in the connection sheet, or save \
             it once with Save config"
        );
    }
    Ok(target)
}

#[cfg(test)]
mod tests;
