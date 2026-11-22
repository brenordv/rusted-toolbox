use alert::alert_app::{exit_code, gpu_hint, run};
use alert::cli_utils::initialize;
use common_cli::tool_exit_helpers::{exit_error, exit_with_code};
use tracing::error;

fn main() {
    let args = initialize();

    match run(args) {
        Ok(window_failure) => {
            if let Some(reason) = &window_failure {
                error!(
                    reason = %reason,
                    hint = %gpu_hint(),
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
