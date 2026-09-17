use crate::cat_app::run;
use crate::cli_utils::initialize;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use common_utils::constants::SIZE_128KB;
use std::io::{self, BufWriter};

mod cat_app;
mod cli_utils;
mod models;

fn main() {
    let options = initialize();

    let stdout = io::stdout();
    let mut out = BufWriter::with_capacity(SIZE_128KB, stdout.lock());

    if run(&options, &mut out) {
        exit_success();
    } else {
        exit_error();
    }
}
