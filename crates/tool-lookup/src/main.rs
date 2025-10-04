use crate::cli_utils::{get_cli_arguments, print_runtime_info};
use crate::lookup_app::lookup_files;
use anyhow::Result;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;

mod cli_utils;
mod lookup_app;
mod models;

fn main() -> Result<()> {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Error);

    let config = get_cli_arguments()?;

    if !config.no_header {
        print_runtime_info(&config);
    }

    lookup_files(&config)?;

    Ok(())
}
