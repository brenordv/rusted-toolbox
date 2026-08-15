mod b64_app;
mod cli_utils;
mod models;

use crate::b64_app::run;
use crate::cli_utils::initialize;
use tracing::error;
use common_cli::tool_exit_helpers::{exit_success, exit_with_code};

fn main() {    
    let config = initialize();
    
    match run(&config) {
        Ok(()) => exit_success(),
        Err(app_error) => {
            if !app_error.message.is_empty() {
                error!("{}", app_error.message);
            }
            exit_with_code(app_error.exit_code);
        }
    }
}