//! Choosing where a run's `config.json` lives (the current folder, or a
//! saved profile with `--profile`) and reading it.

use crate::cli_utils::DataArgs;
use crate::models::ConfigSource;
use anyhow::{Result, bail};
use shared_eventhub_persistence::config::{CONFIG_FILE_NAME, ConfigLoad, ToolConfig, load_config};
use shared_eventhub_persistence::workspace::{ConfigHome, ProfileName, Workspace};
use std::path::PathBuf;

/// A run's config home, the `config.json` recalled from it, and what
/// happened to that file.
pub(crate) type LoadedHome = (ConfigHome, Option<ToolConfig>, ConfigSource);

/// Whether a subcommand offers `--save-config` and whether it was given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Saving {
    /// The subcommand has no `--save-config`.
    NotOffered,
    No,
    /// `--save-config`: a profile that does not exist yet is created.
    Yes,
}

/// The run's config home, the `config.json` recalled from it, and what
/// happened to that file. Without `--profile`, the home is
/// `current_folder` and its `config.json` is read only when `read_config`
/// is set. With `--profile`, the home is the profile `locate` finds, and its
/// `config.json` is always read: clap refuses `--profile` beside
/// `--no-config` and `--dry-run`, the two runs that read none.
///
/// # Errors
/// Fails when the profile name breaks the name rules, when the workspace
/// cannot be found or listed, when the profile does not exist and the run
/// does not save it, when the profile folder's `config.json` belongs to
/// another tool, and when a `config.json` cannot be read or is not valid.
pub(crate) async fn load_home(
    data: &DataArgs,
    saving: Saving,
    read_config: bool,
    current_folder: PathBuf,
    locate: impl FnOnce() -> Result<Workspace>,
) -> Result<LoadedHome> {
    let Some(name) = &data.profile else {
        let (recalled, source) = if read_config {
            recall(load_config(&current_folder).await?)
        } else {
            (None, ConfigSource::Disabled)
        };
        return Ok((ConfigHome::CurrentFolder(current_folder), recalled, source));
    };

    let name = ProfileName::parse(name)?;
    let workspace = locate()?;
    let profile = workspace.profile(&name).await?;
    let loaded = load_config(profile.dir()).await?;
    match (loaded, saving) {
        (ConfigLoad::Loaded(config), Saving::NotOffered | Saving::No | Saving::Yes) => Ok((
            ConfigHome::Profile(profile),
            Some(*config),
            ConfigSource::Loaded,
        )),
        (ConfigLoad::Missing, Saving::Yes) => {
            Ok((ConfigHome::Profile(profile), None, ConfigSource::Missing))
        }
        (ConfigLoad::Missing, Saving::No | Saving::NotOffered) => {
            bail!(no_profile(&name, &workspace, saving))
        }
        (ConfigLoad::Foreign, Saving::NotOffered | Saving::No | Saving::Yes) => bail!(
            "'{}' is not an eh config; a profile folder holds eh's config.json",
            profile.dir().join(CONFIG_FILE_NAME).display()
        ),
    }
}

fn recall(loaded: ConfigLoad) -> (Option<ToolConfig>, ConfigSource) {
    match loaded {
        ConfigLoad::Missing => (None, ConfigSource::Missing),
        ConfigLoad::Foreign => (None, ConfigSource::Foreign),
        ConfigLoad::Loaded(config) => (Some(*config), ConfigSource::Loaded),
    }
}

/// The error for a profile that does not exist, telling a run that can save
/// how to create it.
fn no_profile(name: &ProfileName, workspace: &Workspace, saving: Saving) -> String {
    let name = name.as_str();
    let missing = format!(
        "No profile named {name} in '{}'. `eh profile list` shows the saved ones",
        workspace.root().display()
    );
    match saving {
        Saving::NotOffered => missing,
        Saving::No | Saving::Yes => {
            format!("{missing}; add --save-config to save this run as profile {name}")
        }
    }
}

#[cfg(test)]
mod tests;
