use anyhow::Context;
use common_cli::tool_exit_helpers::exit_with_code;
use dep_check::cli_utils::{TOOL, USER_AGENT, initialize};
use dep_check::dep_check_app::{Deps, RunEnd, exit_code_of, run};
use dep_check_core::engine::{Checker, system_clock};
use std::io::IsTerminal;
use tracing::error;

fn main() {
    let args = match initialize() {
        Ok(args) => args,
        Err(early) => {
            // Logging is not booted yet: the command line ended the run before it started.
            if early.to_stdout {
                println!("{}", early.text);
            } else {
                eprintln!("{}", early.text);
            }
            exit_with_code(early.code);
        }
    };
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to start the async runtime")
        .and_then(|runtime| {
            let checker = Checker::new(TOOL, USER_AGENT)?;
            let stdin = std::io::stdin();
            let deps = Deps {
                stdin_terminal: stdin.is_terminal(),
                stdin: stdin.lock(),
                stdout: std::io::stdout().lock(),
                checker,
                clock: system_clock(),
            };
            runtime.block_on(run(args, deps))
        });
    match &result {
        Ok(RunEnd::Reported(_)) => {}
        Ok(RunEnd::Refused(line)) => {
            error!(reason = %line, "dep-check could not read the packages")
        }
        Err(error) => error!(error = %format!("{error:#}"), "dep-check failed"),
    }
    exit_with_code(exit_code_of(&result).code());
}
