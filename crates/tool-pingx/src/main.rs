mod models;
mod cli_utils;
mod pingx_app;

use anyhow::Result;
use pingx_app::run_ping;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;

#[tokio::main]
async fn main() -> Result<()> {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Error);

    let args = match cli_utils::get_cli_arguments() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    run_ping(&args).await?;
    Ok(())
}