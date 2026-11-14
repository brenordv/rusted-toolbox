//! What the UI asks of the outside world, and the production implementation over the tokio
//! runtime, the archive slot, and Rune's `AppHandle`.

use crate::app::run_effects;
use crate::bridge::{EngineDone, EngineSession, UiDelivery, UiPort, spawn_session};
use crate::state::{AppState, Input};
use crate::tasks::StoreSlot;
use anyhow::Context;
use rune::AppHandle;
use shared_eventhub_core::connection::EventHubTarget;
use shared_eventhub_core::engine::CaptureEngine;
use shared_eventhub_persistence::config::{
    SensitiveValues, ToolConfig, needs_stored_secret, resolve_target_with, save_config,
};
use shared_eventhub_persistence::export::{DiskWriteConfig, WritingSink};
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::secrets::SecretsStore;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use tracing::{debug, error};

/// A request to start a session: the config, the typed secrets, and the data folder it is for.
#[derive(Debug)]
pub struct ConnectRequest {
    pub config: ToolConfig,
    pub given: SensitiveValues,
    pub paths: DataPaths,
}

/// A request to save `config.json`: the config, the typed secrets, and the data folder whose
/// secrets file receives them.
#[derive(Debug)]
pub struct SaveRequest {
    pub config: ToolConfig,
    pub given: SensitiveValues,
    pub paths: DataPaths,
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
    fn reopen_window(&self);
    /// Closes the store, then ends the app.
    fn quit(&self);
}

/// The production services.
pub struct ProdServices {
    handle: AppHandle<AppState>,
    rt: Handle,
    slot: Arc<StoreSlot>,
    working_dir: PathBuf,
}

impl ProdServices {
    /// Builds the services and starts the once-a-second UI tick, which sets `window_failed`
    /// when the window could not be created.
    pub fn new(
        handle: AppHandle<AppState>,
        rt: Handle,
        slot: Arc<StoreSlot>,
        working_dir: PathBuf,
        window_failed: Arc<AtomicBool>,
    ) -> Self {
        spawn_tick(&rt, handle.clone(), window_failed);
        Self {
            handle,
            rt,
            slot,
            working_dir,
        }
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
        let dir = self.working_dir.clone();
        self.rt.spawn(async move {
            let saved = tokio::task::spawn_blocking(move || {
                let secrets = SecretsStore::open(&request.paths)?;
                save_config(&dir, &request.config, &secrets, &request.given)
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
}

/// Applies `input` on the UI thread and runs its effects.
pub(crate) fn apply_from_runtime(handle: &AppHandle<AppState>, input: Input) {
    handle.update_if(move |state, _| {
        let outcome = state.apply(input);
        run_effects(state, outcome.effects);
        outcome.changed
    });
}

/// Posts a tick once a second; ticks that change nothing do not redraw. From the second tick
/// until the window has been seen on screen, it checks for a window failure: Rune keeps running
/// windowless when it cannot create the window (no GPU adapter or device), so that failure is
/// caught here, logged, flagged in `window_failed`, and ends the app.
pub(crate) fn spawn_tick(rt: &Handle, handle: AppHandle<AppState>, window_failed: Arc<AtomicBool>) {
    rt.spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
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
        self.0.update_if(move |state, _| {
            let outcome = state.apply(Input::Delivery(delivery));
            if ack.send(()).is_err() {
                debug!("the forwarder ended before its delivery was applied");
            }
            run_effects(state, outcome.effects);
            outcome.changed
        });
    }

    fn done(&self, done: EngineDone) {
        apply_from_runtime(&self.0, Input::EngineDone(done));
    }
}

/// Opens the store, resolves the target, and starts a session whose output goes to `port`.
/// Always runs to completion in its own task (`StoreSlot::acquire` is not cancel-safe).
pub async fn prepare_session(
    rt: &Handle,
    attempt: u64,
    request: ConnectRequest,
    slot: &StoreSlot,
    port: Arc<dyn UiPort>,
    start: oneshot::Receiver<()>,
) -> anyhow::Result<EngineSession> {
    let store = slot.acquire(&request.paths).await?;
    let target = resolve_target(&request).await?;
    let disk_write = &request.config.storage.disk_write;
    let sink = WritingSink::new(
        store,
        DiskWriteConfig {
            mode: disk_write.mode,
            inbound_dir: request.paths.inbound_dir(&disk_write.received_msg_path),
            content_only: disk_write.content_only,
        },
        request.config.capture.filters.clone(),
    );
    let (channels, run) = CaptureEngine::build(request.config.engine_config(target), sink.clone());
    let finish = async move { sink.finish(|_| {}).await };
    Ok(spawn_session(
        rt,
        attempt,
        gate(start),
        channels,
        run,
        finish,
        port,
    ))
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
        let secrets = SecretsStore::open(&paths)?;
        resolve_target_with(&owned, given, Some(&secrets))
    })
    .await
    .context("Reading the secrets file stopped unexpectedly")??;

    if let (Some(client_id), None) = (config.client_secret_id(), &target.client_secret) {
        anyhow::bail!(
            "No client secret for client '{client_id}': type it in the connection sheet, or save \
             it once with Save config"
        );
    }
    Ok(target)
}

#[cfg(test)]
mod tests;
