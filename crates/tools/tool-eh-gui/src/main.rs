mod cli_utils;

use common_cli::tool_exit_helpers::{exit_error, exit_success};
use rune::{App, AppSpec, Cx, El, col, text};
use tracing::error;

/// The app's state.
#[derive(Default)]
struct Shell;

fn view(_: &Shell, _: &mut Cx<Shell>) -> El {
    col().w_full().h_full().center().child(text("eh-gui"))
}

/// Desktop GUI for Azure Event Hubs.
///
/// Boots logging (file sink on by default), then opens the window. Rune reads no process
/// arguments: clap owns the command line, so Rune's own flags (its headless dumps among them)
/// are refused here like any other unknown flag. Exits 0 on a clean quit and 1 when the event
/// loop cannot start.
fn main() {
    cli_utils::initialize();

    let outcome = App::new(AppSpec::new("eh", (1280.0, 800.0), Shell::default, view))
        .args(Vec::new())
        .try_run();

    match outcome {
        Ok(()) => exit_success(),
        Err(e) => {
            error!(error = %e, "the window could not start");
            exit_error();
        }
    }
}
