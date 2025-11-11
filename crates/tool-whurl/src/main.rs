mod cli_utils;
mod engine;
mod files;
mod includer;
mod models;
mod output;
mod vars;
mod whurl_app;
mod whurl_utils;

use crate::cli_utils::get_cli_arguments;
use crate::whurl_app::{execute, print_error, resolve_log_level};
use shared::logging::logging_helpers::initialize_log;
use shared::system::tool_exit_helpers::exit_with_code;

fn main() {
    let cli = get_cli_arguments();
    let log_level = resolve_log_level(&cli);
    initialize_log(env!("CARGO_PKG_NAME"), log_level);

    if let Err(error) = execute(cli) {
        print_error(&error);
        let code: i32 = error.exit_code().into();
        exit_with_code(code);
    }

    exit_with_code(0);
}
