//! The speak window (`--gui`): Compose, to try a text against the speech server, and
//! Settings, to say where that server is.

pub mod services;
pub mod state;
#[cfg(test)]
pub(crate) mod test_support;
pub mod views;

use crate::config::{self, SpeakConfig};
use crate::design::{WINDOW_MIN, WINDOW_SIZE};
use crate::speech_client::SpeechClient;
use anyhow::{Context, Result};
use common_gui_rune::window::{WatchHooks, run_watched};
use rune::{App, AppSpec};
use services::ProdServices;
use state::SpeakState;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use tokio::runtime::Runtime;
use tracing::info;

/// The request and file work of a window runs on this many threads.
const WORKER_THREADS: usize = 2;

/// The settings at `path` as the window starts with them; `Ok(None)` on a first run.
///
/// # Errors
/// When the file exists and cannot be loaded; the error names the path. The window then never
/// opens, so nothing overwrites a file speak could not read.
pub fn startup_config(runtime: &Runtime, path: &Path) -> Result<Option<SpeakConfig>> {
    let config = runtime.block_on(config::load(path))?;
    if config.is_some() {
        info!(path = %path.display(), "config loaded");
    }
    Ok(config)
}

/// Opens the speak window over the settings at `config_path` and returns when it closes, with
/// the reason when the window could not be created. Rune reads no process arguments: clap
/// owns the command line.
///
/// # Errors
/// When the settings file cannot be loaded, or the runtime, the HTTP client, or the window's
/// event loop cannot start.
pub fn run(config_path: PathBuf) -> Result<Option<String>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(WORKER_THREADS)
        .enable_all()
        .build()
        .context("Failed to start the async runtime")?;
    let config = startup_config(&runtime, &config_path)?;
    let client = SpeechClient::new()?;

    let mut spec = AppSpec::new(
        views::OS_TITLE,
        WINDOW_SIZE,
        SpeakState::detached,
        views::view,
    );
    spec.min_size = Some(WINDOW_MIN);
    spec.on_key = Some(views::window_key);
    let app = App::new(spec);
    let handle = app.handle();
    let rt = runtime.handle().clone();
    let app = app.init(move || {
        let services = ProdServices::new(handle, rt, client);
        SpeakState::new(config, config_path, Some(Rc::new(services)))
    });
    run_watched(
        app,
        WatchHooks {
            shown: views::shown,
            appearance: views::set_appearance,
        },
        "Could not start the speak window",
    )
}

#[cfg(test)]
mod tests;
