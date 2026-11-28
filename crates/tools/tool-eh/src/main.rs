mod cli_utils;
mod dry_run;
mod event_loop;
mod export_app;
mod home;
mod interactive;
mod models;
mod profile_app;
mod progress;
mod read_app;
mod send_app;
mod send_cli;
mod settings;
mod target_setup;

use crate::cli_utils::initialize;
use crate::interactive::Start;
use crate::models::{EhCommand, RunOutcome};
use common_cli::tool_exit_helpers::{exit_error, exit_success, exit_with_code};
use common_signal_monitor::setup_graceful_shutdown::setup_graceful_shutdown;
use common_utils::constants::EXIT_CODE_INTERRUPTED_BY_USER;
use common_utils::string_utils::escape_for_terminal_display;
use eh_core::tls::install_crypto_provider;
use tracing::error;

/// Azure Event Hubs CLI.
///
/// Parses the arguments (or, with none in a terminal, asks for them in a
/// menu) and `config.json`, installs the Ctrl-C handler, and runs `read`,
/// `export`, `send`, a `send --dry-run`, or `profile list`. Exit codes: 0 on
/// success or a cancelled menu, 1 on any failure (including a partition that
/// failed during a stopped run), 130 when interrupted cleanly or by Ctrl-C in
/// the menu.
#[tokio::main]
async fn main() {
    // A build can enable more than one rustls provider, which leaves rustls no default for
    // the Event Hubs connection to use; this sets one before anything connects.
    install_crypto_provider();
    let command = match initialize().await {
        Ok(Start::Run(command)) => command,
        Ok(Start::Cancelled) => {
            eprintln!("Cancelled; nothing ran.");
            exit_success()
        }
        Ok(Start::Interrupted) => exit_with_code(EXIT_CODE_INTERRUPTED_BY_USER),
        Err(error) => fail(&error),
    };

    let shutdown = setup_graceful_shutdown(false);
    let outcome = match command {
        EhCommand::Read(job) => read_app::run(*job, &shutdown).await,
        EhCommand::Export(job) => export_app::run(*job, &shutdown).await,
        EhCommand::Send(job) => send_app::run(*job, &shutdown).await,
        EhCommand::DryRun(job) => dry_run::run(*job, &shutdown),
        EhCommand::ProfileList(workspace) => profile_app::run(&workspace).await,
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
