mod b64_app;
mod cli_utils;
mod models;

use crate::b64_app::{exit_code_for, run};
use crate::cli_utils::initialize;
use common_cli::tool_exit_helpers::{exit_success, exit_with_code};
use tracing::{debug, error};

fn main() {
    let config = initialize();

    match run(&config) {
        Ok(()) => exit_success(),
        Err(error) => {
            let exit_code = exit_code_for(&error);
            if exit_code == 0 {
                debug!("stopping early: output pipe closed by the consumer");
                exit_success();
            } else {
                error!("{:#}", error);
                exit_with_code(exit_code);
            }
        }
    }
}
