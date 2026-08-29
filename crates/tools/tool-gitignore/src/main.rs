use crate::cli_utils::initialize;
use crate::gitignore_app::run_gitignore_maintainer;
use anyhow::Result;

mod cli_utils;
mod config;
mod gitignore_app;
mod models;

#[tokio::main]
async fn main() -> Result<()> {
    let args = initialize()?;

    run_gitignore_maintainer(args.target_folder).await?;

    Ok(())
}
