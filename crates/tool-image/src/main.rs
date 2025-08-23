mod cli_utils;
mod models;
mod image_app;
mod image_edit_routines;
mod image_format_traits;
mod image_encoders;

use anyhow::Result;

use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use crate::cli_utils::{get_cli_arguments, print_runtime_info, validate_args};
use crate::image_app::run_image_edit_commands;

fn main() -> Result<()> {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Info);
    let args = get_cli_arguments();
    validate_args(&args)?;
    print_runtime_info(&args);
    run_image_edit_commands(&args)?;

    Ok(())
}