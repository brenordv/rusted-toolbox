mod cli_utils;
mod models;
mod remove_zw_app;

use crate::cli_utils::{boot, parse_and_build, validate_args};
use crate::remove_zw_app::run;
use common_cli::tool_exit_helpers::{exit_error, exit_success, exit_with_code};
use tracing::error;

/// `--check` exit code when at least one input would be modified, or, under
/// `--fail-on-skip`, when at least one input was skipped and so never verified.
const EXIT_CODE_CHANGES_NEEDED: i32 = 1;
/// `--check` exit code when the run itself failed; distinct from "changes
/// needed" so a pipeline can tell a dirty tree from a broken invocation.
const EXIT_CODE_CHECK_ERROR: i32 = 2;

fn main() {
    let (config, common) = parse_and_build();

    if let Err(e) = validate_args(&config) {
        // Printed directly: the tracing subscriber is not installed until
        // `boot`, so an `error!` here would be dropped.
        eprintln!("Invalid arguments: {e:#}");
        if config.check {
            exit_with_code(EXIT_CODE_CHECK_ERROR);
        }
        exit_error();
    }

    boot(&common, &config);

    match run(&config) {
        Ok(stats)
            if config.check
                && (stats.modified > 0 || (config.fail_on_skip && stats.skipped > 0)) =>
        {
            exit_with_code(EXIT_CODE_CHANGES_NEEDED)
        }
        Ok(_) => exit_success(),
        Err(e) => {
            error!("Failed to remove zero-width characters: {}", e);
            if config.check {
                exit_with_code(EXIT_CODE_CHECK_ERROR);
            }
            exit_error();
        }
    }
}
