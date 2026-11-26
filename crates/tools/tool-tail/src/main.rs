mod cli_utils;
mod models;
mod tail_app;

use crate::cli_utils::initialize;
use common_signal_monitor::setup_graceful_shutdown::setup_graceful_shutdown;
use common_utils::constants::SIZE_128KB;
use head_tail_core::io_shared::exit_from_result;
use std::io::{self, BufWriter};

fn main() {
    let config = initialize();

    let shutdown = setup_graceful_shutdown(false);
    let stdout = io::stdout();
    let mut out = BufWriter::with_capacity(SIZE_128KB, stdout.lock());

    exit_from_result(tail_app::run(&config, &shutdown, &mut out));
}
