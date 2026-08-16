use crate::cat_app::cat_file;
use crate::cli_utils::initialize;
use std::io;
use std::io::Write;
use tracing::error;
use common_cli::tool_exit_helpers::{exit_error, exit_success};

mod cat_app;
mod cli_utils;
mod models;

fn main() {
    let options = initialize();

    if options.files.is_empty() {
        // No files specified, read from stdin
        if let Err(e) = cat_file(None, &options) {
            error!("Failed to run CAT from stdin: {}", e);
            exit_error();
        }
    } else {
        // Process each file
        for filename in &options.files {
            if let Err(e) = cat_file(Some(filename), &options) {
                error!("Failed to run CAT from file [{}]: {}", filename, e);
                exit_error();
            }
        }
    }

    exit_success();
}
