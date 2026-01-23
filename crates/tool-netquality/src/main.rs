mod cli_utils;
mod models;
mod netquality_app;
mod notifiers;
mod persistence;

use crate::cli_utils::get_cli_arguments;
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

    match netquality_app::run_app(&args).await {
        Ok(_) => exit_success(),
        Err(error) => {
            eprintln!("{error}");
            exit_error();
        }
    }
}
