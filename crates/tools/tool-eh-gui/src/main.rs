mod cli_utils;

use anyhow::Context;
use common_cli::app_logger::FileLogging;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use eh_gui::app::{build_app, preload_config};
use eh_gui::services::{GPU_HINT, ProdServices};
use eh_gui::tasks::StoreSlot;
use shared_eventhub_core::tls::install_crypto_provider;
use shared_eventhub_persistence::workspace::Workspace;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{error, warn};

/// Worker threads for the app's runtime: the engine, store work, and the UI tick are light.
const RUNTIME_WORKERS: usize = 2;

/// Desktop GUI for Azure Event Hubs.
///
/// Boots logging (file sink on by default), reads `config.json` from the working directory,
/// finds the profile workspace (eh-gui starts without profiles when there is none), then opens
/// the window. Rune reads no process arguments: clap owns the command line, so Rune's own flags
/// (its headless dumps among them) are refused like any other unknown flag. Exits 0 on a clean
/// quit and 1 when the window cannot start.
fn main() {
    // A build can enable more than one rustls provider, which leaves rustls no default for
    // the Event Hubs connection to use; this sets one before anything connects.
    install_crypto_provider();
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
    let workspace = Workspace::locate().map_err(|error| {
        let reason = format!("{error:#}");
        warn!(reason = %reason, "profiles are unavailable");
        reason
    });
    let preload = runtime.block_on(preload_config(&working_dir, workspace, file_logging));
    let slot = Arc::new(StoreSlot::new(preload.data_paths()));
    let window_failed = Arc::new(AtomicBool::new(false));

    let rt = runtime.handle().clone();
    let failed = Arc::clone(&window_failed);
    let app = build_app(
        move |handle| preload.into_state(Box::new(ProdServices::new(handle, rt, slot, failed))),
        Some(Vec::new()),
    );
    let ran = app.try_run();
    // Dropping a runtime waits without limit for blocking work that has started (an archive
    // scan, an at-end pass the user chose not to wait for), which would keep the process alive
    // with no window. The quit path has already awaited the archive's close, and every stored
    // batch is committed.
    runtime.shutdown_background();
    ran.map_err(|reason| anyhow::anyhow!(reason))
        .with_context(|| format!("Could not start the window; {GPU_HINT}"))?;

    if window_failed.load(Ordering::Acquire) {
        anyhow::bail!("The window could not be created; {GPU_HINT}");
    }
    Ok(())
}
