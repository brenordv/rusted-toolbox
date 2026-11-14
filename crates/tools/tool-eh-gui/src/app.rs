//! Glue between the pure state machine and the running app: the `App` builder both binaries
//! use, the config preload, and the effect runner.

use crate::design::tokens::{TOOLBAR_HEIGHT, WINDOW_SIZE};
use crate::form::ConnectForm;
use crate::services::Services;
use crate::state::{AppState, Effect, Input, ToastKind};
use crate::views;
use common_cli::app_logger::FileLogging;
use rune::{App, AppHandle, AppSpec, Titlebar, Tx};
use shared_eventhub_persistence::config::{ConfigLoad, load_config};
use std::path::Path;
use tracing::{error, warn};

/// What the window starts with: the connect form (recalled from `config.json` when there is
/// one), the caption saying what happened to that file, and the log file's state.
pub struct Preload {
    pub form: ConnectForm,
    pub config_caption: Option<String>,
    pub file_logging: FileLogging,
}

impl Preload {
    /// The first state: these values plus `services`.
    pub fn into_state(self, services: Box<dyn Services>) -> AppState {
        AppState::new(
            Some(services),
            self.form,
            self.config_caption,
            self.file_logging,
        )
    }
}

/// Builds the app. Rune reads the arguments in `rune_args` instead of the process's when
/// given (the shipped app passes none), closing the window keeps the process for a session's
/// at-end pass, and `make_state` builds the first state, services included, before the first
/// frame.
pub fn build_app(
    make_state: impl FnOnce(AppHandle<AppState>) -> AppState + 'static,
    rune_args: Option<Vec<String>>,
) -> App<AppState> {
    let app = App::new(AppSpec::new(
        "eh",
        WINDOW_SIZE,
        AppState::detached,
        views::view,
    ));
    let handle = app.handle();
    let app = match rune_args {
        Some(args) => app.args(args),
        None => app,
    };
    app.titlebar(Titlebar::new(TOOLBAR_HEIGHT))
        .keep_running()
        .on_close(on_close)
        .init(move || make_state(handle))
}

fn on_close(state: &mut AppState, _: &mut Tx) {
    dispatch(state, Input::WindowClosed(chrono::Utc::now()));
}

/// Reads `config.json` from `dir` into the form, with a caption that never quotes a value.
pub async fn preload_config(dir: &Path, file_logging: FileLogging) -> Preload {
    let (form, config_caption) = match load_config(dir).await {
        Ok(ConfigLoad::Missing) => (ConnectForm::default(), None),
        Ok(ConfigLoad::Foreign) => (
            ConnectForm::default(),
            Some("A config.json here belongs to another tool; it was not loaded".to_owned()),
        ),
        Ok(ConfigLoad::Loaded(config)) => {
            let (form, loaded) = ConnectForm::from_config(&config);
            (
                form,
                Some(format!("Loaded from config.json: {}", loaded.join(", "))),
            )
        }
        Err(error) => {
            warn!(error = %format!("{error:#}"), "config.json was not loaded");
            (
                ConnectForm::default(),
                Some(format!("config.json was not loaded: {error:#}")),
            )
        }
    };
    Preload {
        form,
        config_caption,
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
            Effect::ReopenWindow => match &state.services {
                Some(services) => services.reopen_window(),
                None => services_missing(state, "reopen the window"),
            },
            Effect::Quit => match &state.services {
                Some(services) => services.quit(),
                None => services_missing(state, "quit"),
            },
        }
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
