mod cli_utils;

use anyhow::Context;
use common_cli::app_logger::FileLogging;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use eh_gui::app::{build_app, preload_config};
use eh_gui::services::{GPU_HINT, ProdServices};
use eh_gui::tasks::StoreSlot;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::error;

/// Worker threads for the app's runtime: the engine, store work, and the UI tick are light.
const RUNTIME_WORKERS: usize = 2;

/// Desktop GUI for Azure Event Hubs.
///
/// Boots logging (file sink on by default), reads `config.json` from the working directory,
/// then opens the window. Rune reads no process arguments: clap owns the command line, so
/// Rune's own flags (its headless dumps among them) are refused like any other unknown flag.
/// Exits 0 on a clean quit and 1 when the window cannot start.
fn main() {
    let file_logging = cli_utils::initialize();
    match run(file_logging) {
        Ok(()) => exit_success(),
        Err(error) => {
            error!(error = %format!("{error:#}"), "eh-gui failed");
            exit_error();
        }
    }
}

fn run(file_logging: FileLogging) -> anyhow::Result<()> {
    let working_dir = std::env::current_dir().context("Failed to read the working directory")?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(RUNTIME_WORKERS)
        .enable_all()
        .build()
        .context("Failed to start the async runtime")?;
    let preload = runtime.block_on(preload_config(&working_dir, file_logging));
    let slot = Arc::new(StoreSlot::new(preload.form.data_paths()));
    let window_failed = Arc::new(AtomicBool::new(false));

    let rt = runtime.handle().clone();
    let failed = Arc::clone(&window_failed);
    let app = build_app(
        move |handle| {
            preload.into_state(Box::new(ProdServices::new(
                handle,
                rt,
                slot,
                working_dir,
                failed,
            )))
        },
        Some(Vec::new()),
    );
    app.try_run()
        .map_err(|reason| anyhow::anyhow!(reason))
        .with_context(|| format!("Could not start the window; {GPU_HINT}"))?;

    if window_failed.load(Ordering::Acquire) {
        anyhow::bail!("The window could not be created; {GPU_HINT}");
    }
    Ok(())
}
