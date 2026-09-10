use crate::cli_utils::initialize;
use crate::http_app::start_server;

mod cli_utils;
mod http_app;
mod models;

#[tokio::main]
async fn main() {
    let config = initialize();

    start_server(config).await;
}
