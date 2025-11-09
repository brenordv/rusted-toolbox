use crate::cli_utils::get_cli_arguments;
use crate::lookup_files_app::{print_header as print_files_header, run_files_lookup};
use crate::lookup_text_app::{print_header as print_text_header, run_text_lookup};
use crate::models::LookupCommand;
use anyhow::Result;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;

mod cli_utils;
mod lookup_files_app;
mod lookup_shared;
mod lookup_text_app;
mod models;

fn main() -> Result<()> {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Error);

    match get_cli_arguments()? {
        LookupCommand::Text(cfg) => {
            if !cfg.no_header {
                print_text_header(&cfg);
            }
            run_text_lookup(&cfg)?;
        }
        LookupCommand::Files(cfg) => {
            if !cfg.no_header {
                print_files_header(&cfg);
            }
            run_files_lookup(&cfg)?;
        }
    }

    Ok(())
}
