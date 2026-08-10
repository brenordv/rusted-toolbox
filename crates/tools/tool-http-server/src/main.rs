use crate::cli_utils::{get_cli_arguments, print_runtime_info};
use crate::http_app::start_server;
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;

mod cli_utils;
mod http_app;
mod models;

#[tokio::main]
async fn main() {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Info);

    let args = get_cli_arguments();

    print_runtime_info(&args);

    start_server(args).await;
}
