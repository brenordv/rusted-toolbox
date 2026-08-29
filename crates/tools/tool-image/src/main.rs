mod cli_utils;
mod image_app;
mod image_edit_routines;
mod image_encoders;
mod image_format_traits;
mod models;
mod string_traits;

use anyhow::Result;

use crate::cli_utils::initialize;
use crate::image_app::run_image_edit_commands;

fn main() -> Result<()> {
    let args = initialize();
    run_image_edit_commands(&args)?;
    Ok(())
}
