use crate::cli_utils::get_cli_arguments;
use crate::mock_app::generate_mock_data;
use crate::models::{DataType, MockOptions};
use shared::logging::app_logger::LogLevel;
use shared::logging::logging_helpers::initialize_log;
use shared::system::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

mod cli_utils;
mod generators;
mod mock_app;
mod models;

fn main() {
    initialize_log(env!("CARGO_PKG_NAME"), LogLevel::Error);

    let args = get_cli_arguments();

    // Validate arguments and create options
    let options = match MockOptions::from_args(&args) {
        Ok(opts) => opts,
        Err(e) => {
            eprintln!("Error: {}", e);
            eprintln!("\nAvailable data types:\n{}", DataType::all_commands());
            exit_error();
            return; // This line will never be reached, but satisfies the compiler
        }
    };

    // Generate mock data
    match generate_mock_data(&options) {
        Ok(result) => {
            println!("{}", result);
            exit_success();
        }
        Err(e) => {
            error!("Failed to generate mock data: {}", e);
            eprintln!("Error: Failed to generate mock data: {}", e);
            exit_error();
        }
    }
}