mod cli_utils;
mod event_loop;
mod export_app;
mod models;
mod progress;
mod read_app;
mod send_app;
mod send_cli;
mod settings;
mod target_setup;

use crate::cli_utils::initialize;
use crate::models::{EhCommand, RunOutcome};
use cli_signal_monitor::setup_graceful_shutdown::setup_graceful_shutdown;
use common_cli::tool_exit_helpers::{exit_error, exit_success, exit_with_code};
use common_utils::constants::EXIT_CODE_INTERRUPTED_BY_USER;
use common_utils::string_utils::escape_for_terminal_display;
use tracing::error;

/// Azure Event Hubs CLI.
///
/// Parses the arguments and `config.json`, installs the Ctrl-C handler, and
/// runs `read`, `export`, or `send`. Exit codes: 0 on success, 1 on any
/// failure (including a partition that failed during a stopped run), 130
/// when interrupted cleanly.
#[tokio::main]
async fn main() {
    let command = match initialize().await {
        Ok(command) => command,
        Err(error) => fail(&error),
    };

    let shutdown = setup_graceful_shutdown(false);
    let outcome = match command {
        EhCommand::Read(job) => read_app::run(*job, &shutdown).await,
        EhCommand::Export(job) => export_app::run(*job, &shutdown).await,
        EhCommand::Send(job) => send_app::run(*job, &shutdown).await,
    };

    match outcome {
        Ok(RunOutcome::Completed) => exit_success(),
        Ok(RunOutcome::Interrupted) => exit_with_code(EXIT_CODE_INTERRUPTED_BY_USER),
        Err(error) => fail(&error),
    }
}

/// Reports `error` once and exits 1.
fn fail(error: &anyhow::Error) -> ! {
    error!("{}", rendered_error(error));
    exit_error();
}

/// The full error chain on one line, escaped: its text can carry event data
/// or `config.json` values.
fn rendered_error(error: &anyhow::Error) -> String {
    escape_for_terminal_display(&format!("{error:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reported_error_has_its_whole_chain_with_control_bytes_escaped() {
        let error =
            anyhow::anyhow!("folder 'data\u{1b}]52;c;x\u{7}\rfake'").context("Failed to open");

        let rendered = rendered_error(&error);

        assert_eq!(
            rendered,
            "Failed to open: folder 'data\\u{1b}]52;c;x\\u{7}\\rfake'"
        );
    }
}
