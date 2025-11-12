mod b64_app;
mod cli_utils;
mod models;

use crate::b64_app::run;
use crate::cli_utils::get_cli_arguments;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::tool_exit_helpers::{exit_success, exit_with_code};
use tracing::error;

fn main() {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Error);

    let config = get_cli_arguments();

    match run(&config) {
        Ok(()) => exit_success(),
        Err(app_error) => {
            if !app_error.message.is_empty() {
                eprintln!("{}: {}", env!("CARGO_PKG_NAME"), app_error.message);
                error!("{}", app_error.message);
            }
            exit_with_code(app_error.exit_code);
        }
    }
}
