//! Demo mode: the real UI against a generated event hub and a throwaway data folder, for manual
//! and agent-driven testing without a namespace or credentials. Built only with the `demo`
//! feature; the shipped `eh-gui` never contains it.

pub mod hub;
pub mod profiles;
mod seed;
pub mod send;

pub use profiles::seed_profiles;
pub use seed::seed_archive;

use crate::app::dispatch;
use crate::archive::bulk::BulkRequest;
use crate::archive::{ArchiveInput, PageRequest};
use crate::bridge::{EngineSession, SessionSink, UiPort, spawn_session};
use crate::form::ConnectForm;
use crate::read::ExportOneRequest;
use crate::read::search::RebuildRequest;
use crate::send::compose::PacingChoice;
use crate::send::{SendInput, SendRequest};
use crate::services::{
    AppPort, ConnectRequest, CreateRequest, SaveRequest, Services, apply_from_runtime,
    created_profile, filter_hook, gate, listed_profiles, loaded_profile, open_secrets,
    report_ready, spawn_bulk_export, spawn_export_one, spawn_list_flush, spawn_rebuild,
    spawn_search_archive, spawn_tick,
};
use crate::state::{AppState, Input, Section, SheetPurpose, ToastInput, ToastKind};
use crate::tasks::StoreSlot;
use anyhow::Context;
use common_cli::app_logger::FileLogging;
use rune::AppHandle;
use shared_eventhub_persistence::config::save_config;
use shared_eventhub_persistence::export::{DiskWriteConfig, ExportOptions, WritingSink};
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::workspace::Workspace;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use tracing::debug;

/// The environment variable naming the scenario (argv belongs to Rune's agent flags).
pub const SCENARIO_VARIABLE: &str = "EH_GUI_DEMO";
/// A generated connection string for the demo hub; it reaches no service.
pub const DEMO_CONNECTION_STRING: &str = "Endpoint=sb://demo.servicebus.windows.net/;\
    SharedAccessKeyName=demo;SharedAccessKey=ZGVtbw==;EntityPath=demo";
const SLOW_FINISH: Duration = Duration::from_secs(45);
/// Why `profiles-unavailable` has no workspace, worded as eh words a bad `EH_WORKSPACE`.
const NO_DEMO_WORKSPACE: &str = "EH_WORKSPACE must be an absolute folder path; it is 'profiles'";

/// What the demo shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenario {
    /// Four partitions, about 200 messages a second.
    Live,
    /// 20,000 messages a second for 10 s, then quiet.
    Burst,
    /// One message every 40 s.
    Quiet,
    /// Partition 2 fails after 5 s; the run ends failed.
    PartitionFail,
    /// The run cannot start.
    StartFail,
    /// The at-end pass takes 45 s.
    SlowFinish,
    /// First run, disconnected.
    SeededEmpty,
    /// Failed, with a long error.
    SeededFailed,
    /// Failed with a 2,000-character error, its details open.
    ErrorDetails,
    /// The connect sheet open with validation errors.
    SeededConnect,
    /// Connected, 5,000 rows in the list, statistics, a JSON row selected.
    SeededRead,
    /// [`Scenario::SeededRead`] with a regex quick search.
    SeededSearch,
    /// Connected and filled like [`Scenario::SeededRead`], with no selection, over 32
    /// partitions with eight-digit counts and six long partition errors in the session log.
    SeededPartitions,
    /// The Export section over an archive of 6,000 messages, searched for everything.
    SeededExport,
    /// The Send section with a JSON body and a count of 1,000, batched, idle.
    Send,
    /// The Send section sending 10 events one a second.
    SendPaced,
    /// The Send section with Template on: an order template, idle, with its sample.
    SendTemplate,
    /// A failed Read session with three toasts, folded: two long failures and an info one.
    ToastStack,
    /// [`Scenario::ToastStack`] fanned out, as after Alt+T.
    ToastStackOpen,
    /// The launch folder with a typed connection string, over a workspace of two profiles
    /// that load and one that can't.
    Profiles,
    /// [`Scenario::Profiles`] with the sheet open and the picker locked by a cancelled connect.
    ProfilesLocked,
    /// [`Scenario::Profiles`] with the sheet open after a pick of `billing` failed to load.
    ProfilesFailed,
    /// The sheet open with no workspace (a bad `EH_WORKSPACE`).
    ProfilesUnavailable,
}

/// How fast a scenario's hub produces messages.
#[derive(Clone, Copy, Debug)]
pub struct Pace {
    pub every: Duration,
    per_tick: u32,
    /// Messages stop after this long.
    stop_after: Option<Duration>,
}

impl Pace {
    /// Messages to produce on a tick `elapsed` into the run.
    pub fn batch(&self, elapsed: Duration) -> u32 {
        match self.stop_after {
            Some(limit) if elapsed >= limit => 0,
            Some(_) | None => self.per_tick,
        }
    }
}

impl Scenario {
    const NAMES: [(&'static str, Scenario); 23] = [
        ("live", Self::Live),
        ("burst", Self::Burst),
        ("quiet", Self::Quiet),
        ("partition-fail", Self::PartitionFail),
        ("start-fail", Self::StartFail),
        ("slow-finish", Self::SlowFinish),
        ("seeded-empty", Self::SeededEmpty),
        ("seeded-failed", Self::SeededFailed),
        ("error-details", Self::ErrorDetails),
        ("seeded-connect", Self::SeededConnect),
        ("seeded-read", Self::SeededRead),
        ("seeded-search", Self::SeededSearch),
        ("seeded-export", Self::SeededExport),
        ("send", Self::Send),
        ("send-paced", Self::SendPaced),
        ("send-template", Self::SendTemplate),
        ("toast-stack", Self::ToastStack),
        ("toast-stack-open", Self::ToastStackOpen),
        ("seeded-partitions", Self::SeededPartitions),
        ("profiles", Self::Profiles),
        ("profiles-locked", Self::ProfilesLocked),
        ("profiles-failed", Self::ProfilesFailed),
        ("profiles-unavailable", Self::ProfilesUnavailable),
    ];

    /// The scenario named `name`.
    ///
    /// # Errors
    /// An unknown name returns the list of valid ones.
    pub fn parse(name: &str) -> Result<Self, String> {
        Self::NAMES
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, scenario)| *scenario)
            .ok_or_else(|| {
                let names: Vec<&str> = Self::NAMES.iter().map(|(known, _)| *known).collect();
                format!(
                    "Unknown {SCENARIO_VARIABLE} scenario '{name}'. Valid scenarios: {}",
                    names.join(", ")
                )
            })
    }

    /// The scenario in [`SCENARIO_VARIABLE`], `live` when unset.
    ///
    /// # Errors
    /// An unknown or non-UTF-8 value returns the list of valid names.
    pub fn from_env() -> Result<Self, String> {
        match std::env::var(SCENARIO_VARIABLE) {
            Ok(name) => Self::parse(&name),
            Err(std::env::VarError::NotPresent) => Ok(Self::Live),
            Err(std::env::VarError::NotUnicode(_)) => Self::parse("<not UTF-8>"),
        }
    }

    /// Whether the scenario runs the hub after Connect (seeded ones are fixed states).
    pub fn is_live(self) -> bool {
        match self {
            Self::Live
            | Self::Burst
            | Self::Quiet
            | Self::PartitionFail
            | Self::StartFail
            | Self::SlowFinish => true,
            Self::SeededEmpty
            | Self::SeededFailed
            | Self::ErrorDetails
            | Self::SeededConnect
            | Self::SeededRead
            | Self::SeededSearch
            | Self::SeededPartitions
            | Self::SeededExport
            | Self::Send
            | Self::SendPaced
            | Self::SendTemplate
            | Self::ToastStack
            | Self::ToastStackOpen
            | Self::Profiles
            | Self::ProfilesLocked
            | Self::ProfilesFailed
            | Self::ProfilesUnavailable => false,
        }
    }

    /// Whether the once-a-second tick and the list flush run: every live scenario, the seeded
    /// Export state, whose search, paging, and exports are real, and the Send states, whose
    /// sends count up.
    pub fn ticks(self) -> bool {
        match self {
            Self::SeededExport | Self::Send | Self::SendPaced | Self::SendTemplate => true,
            Self::Live
            | Self::Burst
            | Self::Quiet
            | Self::PartitionFail
            | Self::StartFail
            | Self::SlowFinish
            | Self::SeededEmpty
            | Self::SeededFailed
            | Self::ErrorDetails
            | Self::SeededConnect
            | Self::SeededRead
            | Self::SeededSearch
            | Self::SeededPartitions
            | Self::ToastStack
            | Self::ToastStackOpen
            | Self::Profiles
            | Self::ProfilesLocked
            | Self::ProfilesFailed
            | Self::ProfilesUnavailable => self.is_live(),
        }
    }

    /// Whether the archive is seeded before the window opens.
    pub fn seeds_archive(self) -> bool {
        self == Self::SeededExport
    }

    /// Whether the demo's profile workspace is written before the window opens.
    pub fn seeds_profiles(self) -> bool {
        match self {
            Self::Profiles | Self::ProfilesLocked | Self::ProfilesFailed => true,
            Self::Live
            | Self::Burst
            | Self::Quiet
            | Self::PartitionFail
            | Self::StartFail
            | Self::SlowFinish
            | Self::SeededEmpty
            | Self::SeededFailed
            | Self::ErrorDetails
            | Self::SeededConnect
            | Self::SeededRead
            | Self::SeededSearch
            | Self::SeededPartitions
            | Self::SeededExport
            | Self::Send
            | Self::SendPaced
            | Self::SendTemplate
            | Self::ToastStack
            | Self::ToastStackOpen
            | Self::ProfilesUnavailable => false,
        }
    }

    pub fn pace(self) -> Pace {
        let steady = Pace {
            every: Duration::from_millis(20),
            per_tick: 4,
            stop_after: None,
        };
        match self {
            Self::Burst => Pace {
                every: Duration::from_millis(10),
                per_tick: 200,
                stop_after: Some(Duration::from_secs(10)),
            },
            Self::Quiet => Pace {
                every: Duration::from_secs(40),
                per_tick: 1,
                stop_after: None,
            },
            Self::Live
            | Self::PartitionFail
            | Self::StartFail
            | Self::SlowFinish
            | Self::SeededEmpty
            | Self::SeededFailed
            | Self::ErrorDetails
            | Self::SeededConnect
            | Self::SeededRead
            | Self::SeededSearch
            | Self::SeededPartitions
            | Self::SeededExport
            | Self::Send
            | Self::SendPaced
            | Self::SendTemplate
            | Self::ToastStack
            | Self::ToastStackOpen
            | Self::Profiles
            | Self::ProfilesLocked
            | Self::ProfilesFailed
            | Self::ProfilesUnavailable => steady,
        }
    }

    fn seed(self) -> u64 {
        Self::NAMES
            .iter()
            .position(|(_, scenario)| *scenario == self)
            .map_or(1, |index| 0x9E37_79B9_7F4A_7C15 ^ index as u64)
    }
}

/// The demo's data folder inside its temporary root.
pub fn data_paths(root: &Path) -> DataPaths {
    DataPaths::new(root.join("data"))
}

/// The first state of `scenario`. Seeded states are reached by feeding inputs through `apply`,
/// so they are real states rather than hand-assembled ones; a seeded session runs on `rt`.
pub fn initial_state(
    scenario: Scenario,
    root: &Path,
    services: Box<dyn Services>,
    rt: &Handle,
) -> AppState {
    let mut form = ConnectForm::default();
    form.connection_string
        .edit_mut()
        .replace_text(DEMO_CONNECTION_STRING);
    form.data_folder = data_paths(root).base().display().to_string();
    let mut state = AppState::new(
        Some(services),
        form,
        Some("Demo mode: a generated event hub and a temporary data folder".to_owned()),
        FileLogging::Off,
    );
    let workspace = if scenario == Scenario::ProfilesUnavailable {
        Err(NO_DEMO_WORKSPACE.to_owned())
    } else {
        Ok(profiles::workspace(root))
    };
    state.set_launch(root.to_path_buf(), workspace);
    match scenario {
        Scenario::SeededFailed => seed_failure(&mut state, demo_failure()),
        Scenario::ErrorDetails => {
            let failure = vec![demo_failure(); 7].join("\n");
            seed_failure(&mut state, failure.clone());
            drop(state.apply(Input::ShowDetails {
                title: "Couldn't connect".to_owned(),
                text: failure,
                toast: None,
            }));
        }
        Scenario::SeededConnect => {
            state.form.set_auth(crate::form::AuthChoice::KeyVault);
            state.form.credential = crate::form::CredentialChoice::ClientSecret;
            drop(state.apply(Input::OpenSheet(SheetPurpose::Connect)));
            drop(state.apply(Input::ConnectPressed));
        }
        Scenario::SeededRead => seed::read(&mut state, rt, false),
        Scenario::SeededSearch => seed::read(&mut state, rt, true),
        Scenario::SeededPartitions => seed::partitions(&mut state, rt),
        Scenario::SeededExport => {
            state.section = Section::Export;
            dispatch(&mut state, Input::Archive(ArchiveInput::SearchPressed));
        }
        Scenario::Send => {
            compose_demo_send(&mut state, "1000");
            let composer = &mut state.send.composer;
            composer.correlation_id.replace_text("demo-run-7");
            composer.properties.replace_text("source=load-test\nrun=7");
        }
        Scenario::SendPaced => {
            compose_demo_send(&mut state, "10");
            state.send.composer.pacing = PacingChoice::Paced;
            dispatch(&mut state, Input::Send(SendInput::SendPressed));
        }
        Scenario::SendTemplate => {
            compose_demo_send(&mut state, "100");
            let composer = &mut state.send.composer;
            composer.body.replace_text(ORDER_TEMPLATE);
            composer.set_template(true);
        }
        Scenario::ToastStack => seed_toasts(&mut state),
        Scenario::ToastStackOpen => {
            seed_toasts(&mut state);
            drop(state.apply(Input::Toast(ToastInput::Hotkey)));
        }
        Scenario::ProfilesLocked => profiles::locked(&mut state, rt),
        Scenario::ProfilesFailed => profiles::failed(&mut state, rt),
        Scenario::ProfilesUnavailable => profiles::open_listed(&mut state, rt),
        Scenario::Live
        | Scenario::Burst
        | Scenario::Quiet
        | Scenario::PartitionFail
        | Scenario::StartFail
        | Scenario::SlowFinish
        | Scenario::SeededEmpty
        | Scenario::Profiles => {}
    }
    state
}

/// A failed connect, then three toasts, oldest first: a long failure, an export that ended,
/// and another long failure.
fn seed_toasts(state: &mut AppState) {
    seed_failure(state, demo_failure());
    let long = vec![demo_failure(); 7].join(" ");
    state.toast(ToastKind::Danger, format!("Partition 2 failed: {long}"));
    state.toast(
        ToastKind::Info,
        "Exported 1,204 messages to exports/orders".to_owned(),
    );
    state.toast(
        ToastKind::Danger,
        format!("Could not save config.json: {long}"),
    );
}

/// The demo's connect failure: a long, real-looking service refusal.
fn demo_failure() -> String {
    format!(
        "Failed to read the event hub properties of '{}/{}': the service rejected the \
         credential (401 Unauthorized). Check that the shared access policy grants Listen on \
         this event hub, that the key was not rotated, and that the namespace allows public \
         network access from this machine.",
        hub::DEMO_NAMESPACE,
        hub::DEMO_HUB
    )
}

/// `state` after a connect that failed with `failure`. A seeded state starts no session, so
/// the effects of these inputs are not run.
fn seed_failure(state: &mut AppState, failure: String) {
    drop(state.apply(Input::ConnectPressed));
    drop(state.apply(Input::SessionReady {
        attempt: 1,
        result: Err(failure),
    }));
}

/// An order body whose id, customer, email, and time are fresh in every event.
const ORDER_TEMPLATE: &str = "{\n  \"orderId\": {{random.integer min=1000 max=9999}},\n  \
    \"customer\": \"{{person.full-name}}\",\n  \"email\": \"{{person.email}}\",\n  \
    \"placedAt\": \"{{random.datetime past}}\"\n}";

/// The Send section with a JSON order body and `count` events.
fn compose_demo_send(state: &mut AppState, count: &str) {
    state.section = Section::Send;
    let composer = &mut state.send.composer;
    composer
        .body
        .replace_text("{\n  \"orderId\": 1,\n  \"sku\": \"A-17\",\n  \"quantity\": 2\n}");
    composer.content_type.replace_text("application/json");
    composer.count.replace_text(count);
}

/// Services that keep everything under the demo's temporary root and never touch the network:
/// sessions run the generated hub, and Save config writes into the root.
pub struct DemoServices {
    handle: AppHandle<AppState>,
    rt: Handle,
    slot: Arc<StoreSlot>,
    root: PathBuf,
    scenario: Scenario,
}

impl DemoServices {
    /// Builds the services; live scenarios also start the 1 Hz tick.
    pub fn new(
        handle: AppHandle<AppState>,
        rt: Handle,
        slot: Arc<StoreSlot>,
        root: PathBuf,
        scenario: Scenario,
    ) -> Self {
        if scenario.ticks() {
            spawn_tick(&rt, handle.clone(), Arc::new(AtomicBool::new(false)));
            spawn_list_flush(&rt, handle.clone());
        }
        Self {
            handle,
            rt,
            slot,
            root,
            scenario,
        }
    }
}

impl Services for DemoServices {
    fn prepare_session(&self, attempt: u64, request: ConnectRequest) {
        let handle = self.handle.clone();
        let session = DemoSession {
            rt: self.rt.clone(),
            slot: Arc::clone(&self.slot),
            root: self.root.clone(),
            scenario: self.scenario,
        };
        self.rt.spawn(async move {
            let port: Arc<dyn UiPort> = Arc::new(AppPort(handle.clone()));
            let (start, started) = oneshot::channel();
            let result = session
                .prepare(attempt, &request, port, started)
                .await
                .map_err(|error| format!("{error:#}"));
            report_ready(&handle, attempt, result, start);
        });
    }

    /// Saves into [`save_dir`]; the secrets go to the demo's data folder.
    fn save_config(&self, request: SaveRequest) {
        let handle = self.handle.clone();
        let root = self.root.clone();
        let dir = save_dir(&root, &request.dir);
        self.rt.spawn(async move {
            let saved = tokio::task::spawn_blocking(move || {
                open_secrets(&data_paths(&root), |secrets| {
                    save_config(&dir, &request.config, secrets, &request.given)
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
        debug!(
            folder = %paths.base().display(),
            "demo mode keeps its temporary data folder"
        );
    }

    /// Lists `workspace` on the runtime.
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

    /// Saves the new profile into the demo's own workspace, whatever workspace the request
    /// names.
    fn create_profile(&self, request: CreateRequest) {
        let handle = self.handle.clone();
        let request = CreateRequest {
            workspace: profiles::workspace(&self.root),
            ..request
        };
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

    /// Reads the demo's own archive whatever the request names; the destination is the one
    /// the user picked in the save dialog.
    fn export_one(&self, request: ExportOneRequest) {
        let request = ExportOneRequest {
            paths: data_paths(&self.root),
            ..request
        };
        spawn_export_one(
            &self.rt,
            self.handle.clone(),
            Arc::clone(&self.slot),
            request,
        );
    }

    /// Reads the demo's own archive whatever the request names.
    fn search_archive(&self, request: PageRequest) {
        let request = PageRequest {
            paths: data_paths(&self.root),
            ..request
        };
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
            rooted_bulk(&self.root, request),
        );
    }

    /// Counts the request's events up without a hub, following its pacing and cancel flag.
    fn start_send(&self, request: SendRequest) {
        let handle = self.handle.clone();
        self.rt.spawn(send::demo_send(request, move |input| {
            apply_from_runtime(&handle, Input::Send(input));
        }));
    }
}

/// Where a demo Save config writes: `requested` when it lies under `root` and holds no `..`,
/// else `root`.
pub fn save_dir(root: &Path, requested: &Path) -> PathBuf {
    let climbs = requested
        .components()
        .any(|part| part == Component::ParentDir);
    if requested.starts_with(root) && !climbs {
        requested.to_path_buf()
    } else {
        debug!("demo mode saves config.json under its own root");
        root.to_path_buf()
    }
}

/// `request` reading the demo's archive and writing under the demo's root, whatever folders
/// it names.
pub fn rooted_bulk(root: &Path, request: BulkRequest) -> BulkRequest {
    let paths = data_paths(root);
    let folder = paths.export_dir(DEMO_EXPORTS);
    if request.options.export_folder != folder {
        debug!("demo mode writes bulk exports into its own data folder");
    }
    BulkRequest {
        options: ExportOptions {
            export_folder: folder,
            ..request.options
        },
        paths,
        ..request
    }
}

/// Message files go here inside the demo data folder, whatever the form says: a typed path
/// could point outside the temporary root.
const DEMO_INBOUND: &str = "inbound";
/// Bulk exports go here inside the demo data folder, whatever folder was picked.
const DEMO_EXPORTS: &str = "exports";

/// What starting a demo session needs, owned so it can move into a runtime task.
struct DemoSession {
    rt: Handle,
    slot: Arc<StoreSlot>,
    root: PathBuf,
    scenario: Scenario,
}

impl DemoSession {
    /// Opens the demo archive and starts a session on the generated hub once `start` fires. The
    /// request's data folder and message folder are ignored: the demo only ever writes under its
    /// own root.
    async fn prepare(
        &self,
        attempt: u64,
        request: &ConnectRequest,
        port: Arc<dyn UiPort>,
        start: oneshot::Receiver<()>,
    ) -> anyhow::Result<EngineSession> {
        let paths = data_paths(&self.root);
        let store = self.slot.acquire(&paths).await?;
        let disk_write = &request.config.storage.disk_write;
        let filters = request.config.capture.filters.clone();
        let sink = WritingSink::new(
            store,
            DiskWriteConfig {
                mode: disk_write.mode,
                inbound_dir: paths.inbound_dir(DEMO_INBOUND),
                content_only: disk_write.content_only,
            },
            filters.clone(),
        );
        let scenario = self.scenario;
        let (channels, run) = hub::build(scenario, filters, sink.clone());
        let set_filters = filter_hook(&sink);
        let finish = async move {
            if scenario == Scenario::SlowFinish {
                tokio::time::sleep(SLOW_FINISH).await;
            }
            sink.finish(|_| {}).await
        };
        Ok(spawn_session(
            &self.rt,
            attempt,
            gate(start),
            channels,
            run,
            SessionSink {
                finish,
                set_filters,
            },
            port,
        ))
    }
}

#[cfg(test)]
mod tests;
