//! What a run of seal-gui does: loads the settings, starts the worker, runs the window, and
//! waits for the worker once the window is gone.

use crate::config::ConfigStore;
use crate::design::{WINDOW_MIN, WINDOW_SIZE};
use crate::services::{PostGate, ProdServices, dispatch};
use crate::state::{AppState, Input};
use crate::views;
use crate::worker::{self, WorkerMsg};
use anyhow::Context;
use common_cli::app_logger::FileLogging;
use common_gui_rune::window::{WatchHooks, run_watched};
use common_utils::file_system::get_user_home_folder;
use rune::{App, AppSpec};
use std::rc::Rc;
use tracing::info;

/// Opens the seal window and returns once it is closed and the worker has ended, with the
/// reason when the window could not be created. A job still running when the window closes
/// finishes first. Rune reads none of the process's arguments.
///
/// # Errors
/// When the worker thread or the window's event loop cannot start.
pub fn run(file_logging: FileLogging) -> anyhow::Result<Option<String>> {
    info!(
        version = env!("CARGO_PKG_VERSION"),
        log_file = ?log_file(&file_logging),
        "seal-gui started"
    );
    let boot = ConfigStore::boot(get_user_home_folder(), |name| std::env::var_os(name));

    let mut spec = AppSpec::new(
        views::OS_TITLE,
        WINDOW_SIZE,
        AppState::detached,
        views::view,
    );
    spec.min_size = Some(WINDOW_MIN);
    spec.on_key = Some(views::window_key);
    let app = App::new(spec);

    let gate = PostGate::default();
    let posting = gate.clone();
    let handle = app.handle();
    let (submit, join) = worker::spawn(move |message: WorkerMsg| {
        let handle = handle.clone();
        posting.forward(move || {
            handle.update_if(move |s: &mut AppState, tx| dispatch(s, tx, Input::from(message)));
        });
    })
    .context("Failed to start the background worker")?;

    let window_gate = gate.clone();
    let app = app.on_close(views::on_close).init(move || {
        AppState::new(
            (boot.config, boot.status),
            file_logging,
            Some(Rc::new(ProdServices::new(submit, boot.store))),
            Some(window_gate),
        )
    });
    let ran = run_watched(
        app,
        WatchHooks {
            shown: views::shown,
            appearance: views::set_appearance,
        },
        "Could not start the seal window",
    );

    gate.close();
    if let Some(kind) = join.running() {
        info!(
            kind = kind.as_str(),
            "window closed while a job is running; waiting for it"
        );
    }
    join.finish();
    ran
}

/// The log file the boot opened, for the start line.
fn log_file(file_logging: &FileLogging) -> Option<&std::path::Path> {
    match file_logging {
        FileLogging::Writing(path) => Some(path),
        FileLogging::Off | FileLogging::Failed { .. } => None,
    }
}
