mod ai_functions;
mod cli_utils;
mod command_parser;
mod distro_cc_app;
mod distro_detect;
mod distro_map;
mod models;

use crate::cli_utils::get_cli_arguments;
use crate::distro_cc_app::start_distro_cc_app;
use anyhow::Result;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::load_env_variables::load_env_variables;
use shared::system::setup_graceful_shutdown::setup_graceful_shutdown;

#[tokio::main]
async fn main() -> Result<()> {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Error);

    load_env_variables()?;

    let _ = setup_graceful_shutdown(true);

    let runtime_config = get_cli_arguments()?;

    start_distro_cc_app(runtime_config).await?;

    Ok(())
}
