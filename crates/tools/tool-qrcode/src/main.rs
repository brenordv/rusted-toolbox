use crate::cli_utils::{get_cli_arguments, print_runtime_info};
use crate::qrcode_app::generate_qrcode;
use anyhow::Result;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;

mod cli_utils;
mod models;
mod qrcode_app;

fn main() -> Result<()> {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Error);

    let config = get_cli_arguments()?;

    if !config.no_header {
        print_runtime_info(&config);
    }

    generate_qrcode(&config)?;

    Ok(())
}
