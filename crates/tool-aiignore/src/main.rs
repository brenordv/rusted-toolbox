use crate::aiignore_app::run_aiignore_maintainer;
use crate::cli_utils::{get_cli_arguments, print_runtime_info, validate_args};
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::tool_exit_helpers::exit_error;
use tracing::error;

mod aiignore_app;
mod cli_utils;
mod models;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Info);
    let args = get_cli_arguments()?;
    if let Err(e) = validate_args(&args) {
        error!("Cannot proceed: {}", e);
        exit_error();
    }
    print_runtime_info(&args);

    run_aiignore_maintainer(args.target_folder).await?;

    Ok(())
}
