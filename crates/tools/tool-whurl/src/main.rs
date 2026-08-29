mod cli_utils;
mod engine;
mod files;
mod includer;
mod models;
mod output;
mod vars;
mod whurl_app;
mod whurl_utils;

use crate::cli_utils::initialize;
use crate::whurl_app::{execute, print_error, resolve_log_level};
use common_cli::app_logger::AppLogger;
use common_cli::tool_exit_helpers::exit_with_code;

fn main() {
    let cli = initialize();
    let log_level = resolve_log_level(&cli);
    AppLogger::new(env!("CARGO_PKG_NAME"), log_level, false, false, false).init(false);

    if let Err(error) = execute(cli) {
        print_error(&error);
        let code: i32 = error.exit_code().into();
        exit_with_code(code);
    }

    exit_with_code(0);
}
