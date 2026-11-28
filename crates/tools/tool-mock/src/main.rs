use crate::cli_utils::initialize;

use crate::models::FromArgs;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use common_mock_data::generate_mock_data;
use common_mock_data::models::MockOptions;
use tracing::error;

mod cli_utils;
mod models;

fn main() {
    let args = initialize();

    let options = MockOptions::from_args(&args);

    // Generate mock data
    match generate_mock_data(&options) {
        Ok(result) => {
            println!("{}", result);
            exit_success();
        }
        Err(e) => {
            error!("Failed to generate mock data: {}", e);
            exit_error();
        }
    }
}
