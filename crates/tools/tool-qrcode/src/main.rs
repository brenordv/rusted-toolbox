mod cli_utils;
mod models;
mod qrcode_app;

use crate::cli_utils::initialize;
use crate::qrcode_app::generate_qrcode;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

fn main() {
    let config = match initialize() {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to parse arguments: {}", e);
            exit_error();
            unreachable!();
        }
    };

    match generate_qrcode(&config) {
        Ok(()) => exit_success(),
        Err(e) => {
            error!("Failed to generate QR code: {}", e);
            exit_error();
        }
    }
}
