use crate::cli_utils::initialize;
use crate::http_app::start_server;
use common_cli::tool_exit_helpers::exit_error;
use tracing::error;

mod cli_utils;
mod http_app;
mod models;

#[tokio::main]
async fn main() {
    let args = initialize();

    match args {
        Ok(a) => {
            start_server(a).await;
        }
        Err(e) => {
            error!("{}", e);
            exit_error()
        }
    }
}
