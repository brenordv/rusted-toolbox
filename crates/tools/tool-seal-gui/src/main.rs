use common_cli::tool_exit_helpers::{exit_error, exit_success};
use common_gui_rune::window::report_failure;
use seal_gui::cli_utils::initialize;
use seal_gui::seal_gui_app::run;
use tracing::error;

fn main() {
    let file_logging = initialize();

    match run(file_logging) {
        Ok(None) => exit_success(),
        Ok(Some(reason)) => {
            report_failure(env!("CARGO_PKG_NAME"), &reason);
            exit_error();
        }
        Err(error) => {
            error!(step = "startup", error = %format!("{error:#}"), "seal-gui could not start");
            exit_error();
        }
    }
}
