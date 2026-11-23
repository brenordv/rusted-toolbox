use alert::alert_app::{exit_code, run};
use alert::cli_utils::initialize;
use common_cli::tool_exit_helpers::{exit_error, exit_with_code};
use common_gui_rune::window::gpu_hint;
use tracing::error;

fn main() {
    let args = initialize();

    match run(args) {
        Ok(window_failure) => {
            if let Some(reason) = &window_failure {
                error!(
                    reason = %reason,
                    hint = %gpu_hint(env!("CARGO_PKG_NAME")),
                    "alert could not open a window"
                );
            }
            exit_with_code(exit_code(window_failure.as_deref()));
        }
        Err(error) => {
            error!(error = %format!("{error:#}"), "alert failed");
            exit_error();
        }
    }
}
