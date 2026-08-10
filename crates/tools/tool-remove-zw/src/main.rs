mod cli_utils;
mod models;
mod remove_zw_app;

use crate::cli_utils::{get_cli_arguments, print_runtime_info, validate_args};
use crate::remove_zw_app::run;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

fn main() {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Error);

    let args = get_cli_arguments();

    if let Err(err) = validate_args(&args) {
        eprintln!("{}: {}", env!("CARGO_PKG_NAME"), err);
        error!("{}", err);
        exit_error();
    }

    if !args.no_header {
        print_runtime_info(&args);
    }

    match run(&args) {
        Ok(()) => exit_success(),
        Err(err) => {
            eprintln!("{}: {}", env!("CARGO_PKG_NAME"), err);
            error!("{}", err);
            exit_error();
        }
    }
}
