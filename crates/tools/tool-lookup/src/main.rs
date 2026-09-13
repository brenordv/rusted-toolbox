use crate::cli_utils::initialize;
use crate::lookup_files_app::run_files_lookup;
use crate::lookup_text_app::run_text_lookup;
use crate::models::LookupCommand;
use common_cli::broken_pipe::BrokenPipe;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::{debug, error};

mod cli_utils;
mod lookup_files_app;
mod lookup_shared;
mod lookup_text_app;
mod models;

/// Text and filename search tool.
///
/// Parses arguments, then searches file contents (`text`) or filenames
/// (`files`), streaming results to stdout. A consumer closing the output pipe
/// ends the run quietly with a success exit.
fn main() {
    let command = match initialize() {
        Ok(command) => command,
        Err(e) => {
            error!("{:#}", e);
            exit_error();
        }
    };

    let mut stdout = std::io::stdout();
    let result = match command {
        LookupCommand::Text(cfg) => run_text_lookup(&cfg, &mut stdout).map(|_| ()),
        LookupCommand::Files(cfg) => run_files_lookup(&cfg, &mut stdout),
    };

    if let Err(e) = result {
        if e.is::<BrokenPipe>() {
            debug!("stopping early: output pipe closed by the consumer");
            exit_success();
        }
        error!("{:#}", e);
        exit_error();
    }

    exit_success();
}
