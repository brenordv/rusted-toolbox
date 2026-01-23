mod checks;
mod cli_utils;
mod models;
mod netqualify_app;
mod notifiers;
mod persistence;
mod runtime_state;

use crate::cli_utils::cli_utils::get_cli_arguments;
use crate::netqualify_app::run_app;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::tool_exit_helpers::{exit_error, exit_success};

#[tokio::main]
async fn main() {
    let args = match get_cli_arguments() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            exit_error();
            return;
        }
    };

    let log_level = if args.verbose {
        LogLevel::Debug
    } else {
        LogLevel::Info
    };

    initialize_log(env!("CARGO_PKG_NAME"), log_level);

    match run_app(&args).await {
        Ok(_) => exit_success(),
        Err(error) => {
            eprintln!("{error}");
            exit_error();
        }
    }
}
