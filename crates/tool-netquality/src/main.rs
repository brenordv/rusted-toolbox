mod checks;
mod cli_utils;
mod models;
mod netqualify_app;
mod notifiers;
mod persistence;
mod runtime_state;

use crate::cli_utils::cli_utils::get_cli_arguments;
use crate::netqualify_app::run_app;
use anyhow::Result;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log_with_otel;
use shared::system::tool_exit_helpers::{exit_error, exit_success};

#[tokio::main]
async fn main() -> Result<()> {
    let args = match get_cli_arguments() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            exit_error();
            unreachable!();
        }
    };

    let log_level = if args.verbose {
        LogLevel::Debug
    } else {
        LogLevel::Info
    };

    let _otel_guard = initialize_log_with_otel(
        env!("CARGO_PKG_NAME"),
        log_level,
        args.otel_endpoint.as_deref(),
    );

    let result = run_app(&args).await;

    // Drop the OTel guard before exiting so providers flush pending spans/logs.
    // exit_success()/exit_error() call std::process::exit(), which skips destructors.
    drop(_otel_guard);

    match result {
        Ok(_) => exit_success(),
        Err(error) => {
            eprintln!("{error}");
            exit_error();
        }
    };

    Ok(())
}
