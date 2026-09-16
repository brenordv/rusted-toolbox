use anyhow::{Context, Result};
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tracing::error;

fn create_manual_shutdown_handler(immediate_exit: bool) -> Result<Arc<AtomicBool>> {
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = Arc::clone(&shutdown);

    ctrlc::set_handler(move || {
        error!("\n[X] Shutdown signal received, stopping application...");
        if immediate_exit {
            error!("[X] Application terminated by the user...");
            exit_success();
        } else {
            shutdown_clone.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    })
    .context("failed to install the Ctrl-C handler")?;

    Ok(shutdown)
}

/// Installs a Ctrl-C (SIGINT) handler and returns a shared shutdown flag.
///
/// With `immediate_exit` false (every current caller), the handler flips the
/// returned `AtomicBool` to `true` on interrupt; the application polls it (for
/// example, `while !flag.load(Ordering::Relaxed)`) and winds down its own work.
/// The flag starts `false` and only ever transitions to `true`.
///
/// With `immediate_exit` true, the handler exits the process itself, with code 0.
/// A script, therefore, cannot distinguish an interrupt from a normal success by
/// exit code alone; this is intentional while the mode has no callers
/// (`common_utils::constants::EXIT_CODE_INTERRUPTED_BY_USER` is available if an
/// interrupt ever needs a distinct exit code).
///
/// The handler installs once per process. A second call fails to register, and
/// the process then exits via `exit_error` with the failure printed to stderr.
pub fn setup_graceful_shutdown(immediate_exit: bool) -> Arc<AtomicBool> {
    match create_manual_shutdown_handler(immediate_exit) {
        Ok(signal) => signal,
        Err(e) => {
            error!("Failed to setup graceful shutdown: {:#}", e);
            exit_error();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    // This is the only test that installs the handler, since the ctrlc handler is
    // once-per-process. The signal-delivery path is not unit tested (delivering a
    // real SIGINT would kill the test runner), and neither is the error path (it
    // calls the diverging `exit_error`).
    #[test]
    fn setup_graceful_shutdown_returns_an_unset_flag() {
        let flag = setup_graceful_shutdown(false);

        assert!(!flag.load(Ordering::Relaxed));
    }
}
