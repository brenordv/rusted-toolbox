//! Glue between the pure state machine and the running app: the `App` builder both binaries
//! use, the config preload, and the effect runner.

use crate::archive::ArchiveEffect;
use crate::design::tokens::{WINDOW_MIN, WINDOW_SIZE};
use crate::read::ReadEffect;
use crate::send::SendEffect;
use crate::services::Services;
use crate::state::{AppState, Effect, Input, Motion, Outcome, Recalled, ToastKind, recalled};
use crate::views;
use common_cli::app_logger::FileLogging;
use rune::{App, AppHandle, AppSpec, Tx};
use shared_eventhub_persistence::config::load_config;
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::workspace::{ConfigHome, Workspace};
use std::path::{Path, PathBuf};
use tracing::{debug, error};

/// What the window starts with: what the launch folder's `config.json` gave (the form, the
/// capture rules, the export settings, and the captions saying what happened to the file and
/// which of its folders were not used), the launch folder, the workspace or why there is none,
/// and the log file's state.
pub struct Preload {
    pub recalled: Recalled,
    pub launch_folder: PathBuf,
    pub workspace: Result<Workspace, String>,
    pub file_logging: FileLogging,
}

impl Preload {
    /// The data folder the recalled form names from the launch folder.
    pub fn data_paths(&self) -> DataPaths {
        ConfigHome::CurrentFolder(self.launch_folder.clone())
            .data_paths(self.recalled.form.data_folder.trim())
    }

    /// The first state: these values plus `services`.
    pub fn into_state(self, services: Box<dyn Services>) -> AppState {
        let Recalled {
            form,
            rules,
            export,
            caption,
            folders_caption,
        } = self.recalled;
        let mut state = AppState::new(Some(services), form, caption, self.file_logging);
        state.folders_caption = folders_caption;
        state
            .read
            .reseed(&rules, export.format, export.include_metadata);
        state.archive.reseed(&export);
        state.set_launch(self.launch_folder, self.workspace);
        state
    }
}

/// Builds the app. Rune reads the arguments in `rune_args` instead of the process's when
/// given (the shipped app passes none), closing the window keeps the process for a session's
/// at-end pass, the window never gets smaller than [`WINDOW_MIN`], and `make_state` builds the
/// first state, services included, before the first frame.
pub fn build_app(
    make_state: impl FnOnce(AppHandle<AppState>) -> AppState + 'static,
    rune_args: Option<Vec<String>>,
) -> App<AppState> {
    let mut spec = AppSpec::new("eh", WINDOW_SIZE, AppState::detached, views::view);
    spec.min_size = Some(WINDOW_MIN);
    spec.on_key = Some(views::window_key);
    let app = App::new(spec);
    let handle = app.handle();
    let app = match rune_args {
        Some(args) => app.args(args),
        None => app,
    };
    with_frame(app)
        .keep_running()
        .on_close(on_close)
        .init(move || make_state(handle))
}

/// macOS: the toolbar is the titlebar, with the traffic lights over it.
#[cfg(target_os = "macos")]
fn with_frame(app: App<AppState>) -> App<AppState> {
    use crate::design::tokens::TOOLBAR_HEIGHT;
    use rune::Titlebar;
    debug!("window frame: the toolbar is the titlebar");
    app.titlebar(Titlebar::new(TOOLBAR_HEIGHT))
}

/// Elsewhere the system title bar stays: on Windows 10, Rune's own titlebar leaves a native
/// caption above it whose close button does nothing.
#[cfg(not(target_os = "macos"))]
fn with_frame(app: App<AppState>) -> App<AppState> {
    debug!("window frame: the system title bar");
    app
}

pub(crate) fn on_close(state: &mut AppState, tx: &mut Tx) {
    let opener = state.toast_hold();
    dispatch(state, Input::WindowClosed(chrono::Utc::now()));
    views::shell::return_toast_focus(state, tx, opener);
}

/// Reads `config.json` from `launch_folder` into the form, with a caption that never quotes a
/// value; `workspace` is where the profiles are, or why there are none.
pub async fn preload_config(
    launch_folder: &Path,
    workspace: Result<Workspace, String>,
    file_logging: FileLogging,
) -> Preload {
    let home = ConfigHome::CurrentFolder(launch_folder.to_path_buf());
    let load = load_config(launch_folder)
        .await
        .map_err(|error| format!("{error:#}"));
    Preload {
        recalled: recalled(&home, load),
        launch_folder: launch_folder.to_path_buf(),
        workspace,
        file_logging,
    }
}

/// Applies `input` to `state` and runs the effects it returns.
pub fn dispatch(state: &mut AppState, input: Input) {
    let outcome = state.apply(input);
    run_effects(state, outcome.effects);
}

/// Carries out the effects a transition returned. Background work goes through the state's
/// services; stopping a session only queues a command, so nothing here blocks the UI thread.
pub fn run_effects(state: &mut AppState, effects: Vec<Effect>) {
    for effect in effects {
        match effect {
            Effect::StopSession => {
                if let Some(session) = state.connection.session() {
                    session.stop();
                }
            }
            Effect::StopOrphan(session) => session.stop(),
            Effect::PrepareSession { attempt, request } => match &state.services {
                Some(services) => services.prepare_session(attempt, request),
                None => services_missing(state, "connect"),
            },
            Effect::SaveConfig(request) => match &state.services {
                Some(services) => services.save_config(request),
                None => services_missing(state, "save the config"),
            },
            Effect::SwitchDataFolder(paths) => match &state.services {
                Some(services) => services.switch_data_folder(paths),
                None => services_missing(state, "switch the data folder"),
            },
            Effect::ListProfiles { request, workspace } => match &state.services {
                Some(services) => services.list_profiles(request, workspace),
                None => services_missing(state, "list the profiles"),
            },
            Effect::LoadProfile { request, dir } => match &state.services {
                Some(services) => services.load_profile(request, dir),
                None => services_missing(state, "open the profile"),
            },
            Effect::CreateProfile(request) => match &state.services {
                Some(services) => services.create_profile(request),
                None => services_missing(state, "save the profile"),
            },
            Effect::ReopenWindow => match &state.services {
                Some(services) => services.reopen_window(),
                None => services_missing(state, "reopen the window"),
            },
            Effect::Quit => match &state.services {
                Some(services) => services.quit(),
                None => services_missing(state, "quit"),
            },
            Effect::Read(ReadEffect::UpdateFilters(filters)) => match state.connection.session() {
                Some(session) => session.update_filters(filters),
                None => debug!("capture rules not sent: the session already ended"),
            },
            Effect::Read(ReadEffect::RebuildSearch(request)) => match &state.services {
                Some(services) => services.rebuild_search(request),
                None => services_missing(state, "filter the list"),
            },
            Effect::Read(ReadEffect::ExportOne(request))
            | Effect::Archive(ArchiveEffect::ExportOne(request)) => match &state.services {
                Some(services) => services.export_one(request),
                None => services_missing(state, "export the message"),
            },
            Effect::Archive(ArchiveEffect::Search(request)) => match &state.services {
                Some(services) => services.search_archive(request),
                None => services_missing(state, "search the archive"),
            },
            Effect::Archive(ArchiveEffect::BulkExport(request)) => match &state.services {
                Some(services) => services.bulk_export(request),
                None => services_missing(state, "export the matches"),
            },
            Effect::Send(SendEffect::StartSend(request)) => match &state.services {
                Some(services) => services.start_send(request),
                None => services_missing(state, "send the events"),
            },
        }
    }
}

/// Applies a commit's motion: rows landing never move, a tick's numbers and the first
/// connect animate even though no input caused them.
pub fn apply_motion(outcome: &Outcome, tx: &mut Tx) {
    match outcome.motion {
        Motion::Still => {}
        Motion::Animated => tx.animated(),
        Motion::Instant => tx.instant(),
    }
}

/// The state runs without services only between `AppSpec`'s constructor and `App::init`, which
/// no input can reach; this reports it instead of panicking should that ever change.
fn services_missing(state: &mut AppState, action: &str) {
    error!(action, "the app's services are not set up");
    state.toast(
        ToastKind::Danger,
        format!("Internal error: eh-gui could not {action}"),
    );
}

#[cfg(test)]
mod tests;
