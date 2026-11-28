//! dep-check-gui: a window that checks the packages of one ecosystem against OSV.dev, deps.dev,
//! and the GitHub Advisory Database, on Rune.

pub mod cli_utils;
pub mod design;
pub mod services;
pub mod state;
#[cfg(test)]
pub(crate) mod test_support;
pub mod views;

use anyhow::{Context, Result};
use common_cli::app_logger::FileLogging;
use common_gui_rune::window::{WatchHooks, run_watched};
use dep_check_core::engine::Checker;
use design::{WINDOW_MIN, WINDOW_SIZE};
use rune::{App, AppSpec};
use services::ProdServices;
use state::DepCheckState;
use std::rc::Rc;
use std::sync::Arc;
use tracing::info;

/// The tool and its version, as the report names it.
pub const TOOL: &str = concat!("dep-check-gui ", env!("CARGO_PKG_VERSION"));
/// The `User-Agent` of every request.
pub const USER_AGENT: &str = concat!(
    "dep-check-gui/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/brenordv/rusted-toolbox)"
);

/// The checks run on this many threads.
const WORKER_THREADS: usize = 2;

/// Opens the dep-check window and returns once it is closed, with the reason when the window
/// could not be created. A check still running when the window closes is dropped: its report
/// was only for the screen, and each source's rate-limit change is saved as that source
/// finishes. Rune reads none of the process's arguments.
///
/// # Errors
/// When the runtime, the HTTP clients, or the window's event loop cannot start.
pub fn run(file_logging: FileLogging) -> Result<Option<String>> {
    info!(
        version = env!("CARGO_PKG_VERSION"),
        log_file = ?log_file(&file_logging),
        "dep-check-gui started"
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(WORKER_THREADS)
        .enable_all()
        .build()
        .context("Failed to start the async runtime")?;
    let checker = Arc::new(Checker::new(TOOL, USER_AGENT).context("Failed to set up the checks")?);

    let mut spec = AppSpec::new(
        views::OS_TITLE,
        WINDOW_SIZE,
        DepCheckState::detached,
        views::view,
    );
    spec.min_size = Some(WINDOW_MIN);
    spec.on_key = Some(views::window_key);
    let app = App::new(spec);
    let handle = app.handle();
    let rt = runtime.handle().clone();
    let app = app
        .on_close(views::on_close)
        .init(move || DepCheckState::new(Some(Rc::new(ProdServices::new(handle, rt, checker)))));
    let ran = run_watched(
        app,
        WatchHooks {
            shown: views::shown,
            appearance: views::set_appearance,
        },
        "Could not start the dep-check window",
    );
    // Without waiting for a check still running: dropping the runtime would wait for it.
    runtime.shutdown_background();
    ran
}

/// The log file the boot opened, for the start line.
fn log_file(file_logging: &FileLogging) -> Option<&std::path::Path> {
    match file_logging {
        FileLogging::Writing(path) => Some(path),
        FileLogging::Off | FileLogging::Failed { .. } => None,
    }
}
