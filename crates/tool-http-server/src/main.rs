use crate::cli_utils::{get_cli_arguments, print_runtime_info};
use crate::http_app::start_server;

mod cli_utils;
mod http_app;
mod models;

#[tokio::main]
async fn main() {
    let args = get_cli_arguments();

    print_runtime_info(&args);

    start_server(args).await;
}
