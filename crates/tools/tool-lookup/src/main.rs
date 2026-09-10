use crate::cli_utils::initialize;
use crate::lookup_files_app::run_files_lookup;
use crate::lookup_text_app::run_text_lookup;
use crate::models::LookupCommand;
use anyhow::Result;

mod cli_utils;
mod lookup_files_app;
mod lookup_shared;
mod lookup_text_app;
mod models;

fn main() -> Result<()> {
    match initialize()? {
        LookupCommand::Text(cfg) => {
            run_text_lookup(&cfg)?;
        }
        LookupCommand::Files(cfg) => run_files_lookup(&cfg)?,
    }

    Ok(())
}
