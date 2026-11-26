//! `eh profile list`: the saved profiles in the workspace.

use crate::cli_utils::item;
use crate::models::RunOutcome;
use crate::progress::{describe_auth, display, target_label};
use anyhow::Context;
use clap::{Args, Subcommand};
use common_cli::broken_pipe::{BrokenPipe, flush_out, write_out};
use eh_persistence::config::{AuthMethodConfig, ToolConfig};
use eh_persistence::workspace::{ProfileEntry, ProfileState, Workspace};
use std::io::{self, Write};
use std::path::Path;

/// `eh profile` arguments.
#[derive(Args, Debug)]
pub(crate) struct ProfileArgs {
    #[command(subcommand)]
    pub command: ProfileCommand,
}

/// `eh profile` subcommands.
#[derive(Subcommand, Debug)]
pub(crate) enum ProfileCommand {
    /// List the saved profiles: name, target, and auth method
    List,
}

/// Prints every profile in `workspace` to stdout, or a line saying there are
/// none yet.
///
/// # Errors
/// Fails when the workspace exists but cannot be listed, and when stdout
/// fails for any reason but a closed reader.
pub async fn run(workspace: &Workspace) -> anyhow::Result<RunOutcome> {
    let entries = workspace.list().await?;
    print_list(
        &mut io::stdout().lock(),
        &list_text(workspace.root(), &entries),
    )?;
    Ok(RunOutcome::Completed)
}

/// The `--app-header` lines for `profile list`.
pub(crate) fn profile_header(workspace: &Workspace) -> Vec<String> {
    vec![
        item("Command", "profile list"),
        item("Workspace", workspace.root().display()),
    ]
}

/// The listing: one line per profile, names aligned, every value escaped.
pub(crate) fn list_text(root: &Path, entries: &[ProfileEntry]) -> String {
    if entries.is_empty() {
        return format!("{}\n", no_profiles(root));
    }
    let root = display(root.display().to_string());
    let names = aligned_names(entries.iter().map(|entry| entry.profile.name()));
    let mut text = format!("Profiles in '{root}':\n");
    for (name, entry) in names.iter().zip(entries) {
        let details = match &entry.state {
            ProfileState::Ready(config) => describe(config),
            ProfileState::CannotLoad(reason) => format!("can't load: {}", display(reason)),
        };
        text.push_str(&format!("  {name}  {details}\n"));
    }
    text
}

/// Each name escaped for the terminal and padded to the longest, so text
/// after the names lines up.
pub(crate) fn aligned_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let names: Vec<String> = names.into_iter().map(display).collect();
    let width = names
        .iter()
        .map(|name| name.chars().count())
        .max()
        .unwrap_or(0);
    names
        .into_iter()
        .map(|name| format!("{name:<width$}"))
        .collect()
}

/// The line for a workspace with no profiles, saying how to save one.
pub(crate) fn no_profiles(root: &Path) -> String {
    format!(
        "No profiles in '{}' yet. Save one with: eh read --profile NAME --save-config <target \
         flags>",
        display(root.display().to_string())
    )
}

/// A ready profile's target and auth method, in the config notice's words.
fn describe(config: &ToolConfig) -> String {
    let auth = match &config.target.auth {
        AuthMethodConfig::ConnectionString { .. } => "connection string (stored)".to_owned(),
        auth @ (AuthMethodConfig::Credential { .. }
        | AuthMethodConfig::KeyVaultConnectionString { .. }) => describe_auth(auth),
    };
    format!("{}, auth {auth}", target_label(&config.target))
}

/// Writes `text` to `out`. A reader that stopped reading (`| head`) got what
/// it wanted, so a closed pipe is not a failure.
///
/// # Errors
/// Fails when a write or the flush fails with anything but a closed pipe.
fn print_list(out: &mut impl Write, text: &str) -> anyhow::Result<()> {
    match write_out(out, text.as_bytes()).and_then(|()| flush_out(out)) {
        Ok(()) => Ok(()),
        Err(error) if error.is::<BrokenPipe>() => Ok(()),
        Err(error) => Err(error).context("Failed to print the profile list"),
    }
}

#[cfg(test)]
mod tests;
