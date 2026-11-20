//! `eh` with no arguments in a terminal: the menu that builds a command
//! line, and how the menu's ending becomes the way the run starts.

mod menu;
mod prompter;
#[cfg(test)]
mod scripted;

use crate::cli_utils::CliArgs;
use crate::progress::display;
use anyhow::Context;
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::tool_log_level::ToolLogLevel;
use menu::{MenuContext, MenuOutcome};
use prompter::{Prompter, TerminalPrompter};
use shared_eventhub_persistence::workspace::{ProfileState, Workspace};
use std::ffi::OsString;
use std::io::{self, IsTerminal, Write};

/// How a run starts.
pub(crate) enum Start<T> {
    Run(T),
    /// The menu was left with Esc or a No at its confirm; nothing ran.
    Cancelled,
    /// The menu was left with Ctrl-C; nothing ran.
    Interrupted,
}

/// Which of the process's standard streams are terminals.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Terminals {
    pub stdin: bool,
    pub stdout: bool,
    pub stderr: bool,
}

impl Terminals {
    fn of_process() -> Self {
        Self {
            stdin: io::stdin().is_terminal(),
            stdout: io::stdout().is_terminal(),
            stderr: io::stderr().is_terminal(),
        }
    }
}

/// Whether this run opens the menu: no arguments after the program name, and
/// stdin, stdout, and stderr all terminals. The prompts read keys and draw on
/// stderr, so a redirected stderr would hide a menu that still waits for keys.
pub(crate) fn wanted(argument_count: usize, terminals: Terminals) -> bool {
    argument_count == 1 && terminals.stdin && terminals.stdout && terminals.stderr
}

/// The command line to run: the process's own, or, for a bare `eh` in a
/// terminal, the one the menu builds. When the menu can't open, its line goes
/// to stderr and the empty command line gets clap's help and exit 2, as in a
/// pipe.
///
/// # Errors
/// Fails when the menu fails; logging is booted first, so the error reaches
/// stderr.
pub(crate) async fn command_line() -> anyhow::Result<Start<CliArgs>> {
    if !wanted(std::env::args_os().len(), Terminals::of_process()) {
        return Ok(Start::Run(CliArgs::parse()));
    }
    let opened = open(
        &mut TerminalPrompter,
        Workspace::locate,
        &mut io::stderr(),
        boot_with_defaults,
    )
    .await?;
    Ok(match opened {
        MenuOutcome::Run(args) => Start::Run(args),
        MenuOutcome::Cancelled => Start::Cancelled,
        MenuOutcome::Interrupted => Start::Interrupted,
        MenuOutcome::Unavailable(line) => {
            eprintln!("{line}");
            Start::Run(CliArgs::parse())
        }
    })
}

/// Gathers the profiles of the workspace `locate` finds, runs the menu, and
/// parses the command line it builds. A workspace that can't be found or
/// listed makes the menu `Unavailable`, with the reason as its line. `boot`
/// runs before an error returns.
///
/// # Errors
/// Fails when the menu fails, and when it builds a command line eh can't
/// parse.
async fn open(
    prompter: &mut impl Prompter,
    locate: impl FnOnce() -> anyhow::Result<Workspace>,
    out: &mut impl Write,
    boot: impl FnOnce(),
) -> anyhow::Result<MenuOutcome<CliArgs>> {
    menu_stage(prompter, locate, out)
        .await
        .inspect_err(|_| boot())
}

async fn menu_stage(
    prompter: &mut impl Prompter,
    locate: impl FnOnce() -> anyhow::Result<Workspace>,
    out: &mut impl Write,
) -> anyhow::Result<MenuOutcome<CliArgs>> {
    let context = match gather(locate).await {
        Ok(context) => context,
        Err(error) => return Ok(MenuOutcome::Unavailable(display(format!("{error:#}")))),
    };
    Ok(match menu::run(prompter, &context, out)? {
        MenuOutcome::Run(tokens) => MenuOutcome::Run(parse(tokens)?),
        MenuOutcome::Cancelled => MenuOutcome::Cancelled,
        MenuOutcome::Interrupted => MenuOutcome::Interrupted,
        MenuOutcome::Unavailable(line) => MenuOutcome::Unavailable(line),
    })
}

/// The workspace's profiles, split into the ones that load and the names of
/// the ones that don't.
async fn gather(locate: impl FnOnce() -> anyhow::Result<Workspace>) -> anyhow::Result<MenuContext> {
    let workspace = locate()?;
    let mut profiles = Vec::new();
    let mut cannot_load = Vec::new();
    for entry in workspace.list().await? {
        match entry.state {
            ProfileState::Ready(config) => {
                profiles.push((entry.profile.name().to_owned(), *config))
            }
            ProfileState::CannotLoad(_) => cannot_load.push(entry.profile.name().to_owned()),
        }
    }
    Ok(MenuContext {
        root: workspace.root().to_path_buf(),
        profiles,
        cannot_load,
    })
}

/// Every value the menu passes was checked before, so a parse error here is
/// the menu's own fault.
fn parse(tokens: Vec<OsString>) -> anyhow::Result<CliArgs> {
    CliArgs::try_parse_from(tokens).context("The menu built arguments eh could not parse")
}

/// Boots logging the way a run with no flags would: warn and above, to
/// stderr, no header.
fn boot_with_defaults() {
    let defaults = CommonToolArgs {
        app_header: false,
        verbose: false,
        default_logging_level: ToolLogLevel::Warn,
        log_to_stdout: false,
        log_to_file: false,
        rotate_log_file_by_day: false,
    };
    defaults.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        true,
        false,
        None::<fn()>,
    );
}

#[cfg(test)]
mod tests;
