//! eh-gui demo mode: the real UI against a generated event hub and a temporary data folder.
//!
//! The scenario comes from `EH_GUI_DEMO` (default `live`). Rune reads this binary's arguments,
//! so its agent flags work in debug builds: `--snap out.png`, `--script file.json`,
//! `--tree tree.json`, `--trace run.jsonl`, `--scene dark`. The data folder is deleted on exit.

use anyhow::Context;
use common_cli::app_logger::AppLogger;
use common_cli::tool_log_level::ToolLogLevel;
use eh_gui::app::build_app;
use eh_gui::demo::{self, DemoServices, Scenario};
use eh_gui::tasks::StoreSlot;
use std::process::ExitCode;
use std::sync::Arc;
use tracing::error;

fn main() -> ExitCode {
    let scenario = match Scenario::from_env() {
        Ok(scenario) => scenario,
        Err(text) => {
            eprintln!("eh-gui-demo: {text}");
            return ExitCode::FAILURE;
        }
    };
    AppLogger::new("eh-gui-demo", ToolLogLevel::Info, false, false, false).init(false);

    match run(scenario) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            error!(error = %format!("{failure:#}"), "eh-gui-demo failed");
            eprintln!("eh-gui-demo: {failure:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(scenario: Scenario) -> anyhow::Result<()> {
    let root = tempfile::tempdir().context("Failed to create the demo data folder")?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .context("Failed to start the async runtime")?;
    let slot = Arc::new(StoreSlot::new(demo::data_paths(root.path())));
    let rt = runtime.handle().clone();
    let path = root.path().to_path_buf();

    let app = build_app(
        move |handle| {
            let services = DemoServices::new(handle, rt, slot, path.clone(), scenario);
            demo::initial_state(scenario, &path, Box::new(services))
        },
        None,
    );
    app.try_run()
        .map_err(|reason| anyhow::anyhow!(reason))
        .context("Could not start the window")
}
