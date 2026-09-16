use crate::cli_utils::initialize;
use crate::gitignore_app::run_gitignore_maintainer;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

mod cli_utils;
mod config;
mod gitignore_app;
mod models;

#[tokio::main]
async fn main() {
    let args = initialize();

    match run_gitignore_maintainer(args).await {
        Ok(()) => exit_success(),
        Err(e) => {
            error!("{:#}", e);
            exit_error();
        }
    }
}
