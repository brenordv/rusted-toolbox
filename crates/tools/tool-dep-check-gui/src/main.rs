use common_cli::tool_exit_helpers::{exit_error, exit_success};
use common_gui_rune::window::report_failure;
use dep_check_gui::cli_utils::initialize;
use dep_check_gui::run;
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
            error!(step = "startup", error = %format!("{error:#}"), "dep-check-gui could not start");
            exit_error();
        }
    }
}
