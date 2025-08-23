mod cli_utils;
mod image_app;
mod image_edit_routines;
mod image_encoders;
mod image_format_traits;
mod models;
mod string_traits;

use anyhow::Result;

use crate::cli_utils::{get_cli_arguments, print_runtime_info, validate_args};
use crate::image_app::run_image_edit_commands;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;

fn main() -> Result<()> {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Error);
    let args = get_cli_arguments();
    validate_args(&args)?;
    print_runtime_info(&args);

    println!("Working:");
    run_image_edit_commands(&args)?;
    Ok(())
}