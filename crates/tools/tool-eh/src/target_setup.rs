//! Turning a run's merged configuration into the event hub target it
//! connects to, saving `config.json` first when asked.

use crate::models::CLIENT_SECRET_ENV;
use crate::progress::{Renderer, display};
use eh_core::connection::EventHubTarget;
use eh_persistence::config::{
    SensitiveValues, ToolConfig, needs_stored_secret, resolve_target_with, save_config,
    with_typed_names,
};
use eh_persistence::paths::DataPaths;
use eh_persistence::secrets::SecretsStore;
use eh_persistence::workspace::ConfigHome;
use std::borrow::Cow;
use std::io::Write;

/// What [`prepare_target`] works from.
pub(crate) struct TargetSetup<'a> {
    pub config: &'a ToolConfig,
    /// Secrets given by flag or environment.
    pub given: &'a SensitiveValues,
    pub paths: &'a DataPaths,
    pub save_config: bool,
    /// Where `config.json` is saved.
    pub home: &'a ConfigHome,
}

/// Builds the event hub target, reading the secrets file only when a secret
/// the auth method uses was not given, and saves the config first when
/// `save_config` is set.
///
/// # Errors
/// Fails when the secrets file cannot be opened or read, when saving fails,
/// when a stored connection string is missing, when a client-secret
/// credential has no secret, or when a profile is saved with a connection
/// string whose event hub cannot be worked out (nothing is written then).
pub(crate) fn prepare_target<O: Write, E: Write>(
    setup: &TargetSetup<'_>,
    renderer: &mut Renderer<O, E>,
) -> anyhow::Result<EventHubTarget> {
    let config = setup.config;
    if !setup.save_config && !needs_stored_secret(config, setup.given) {
        return resolve_target_with(config, setup.given.clone(), None);
    }

    let to_save = if setup.save_config {
        Some(config_to_save(setup)?)
    } else {
        None
    };
    let secrets = SecretsStore::open(setup.paths)?;
    if let Some(to_save) = &to_save {
        save_config(setup.home.dir(), to_save, &secrets, setup.given)?;
        renderer.notice(&saved_notice(setup.given, setup.paths, setup.home));
    }
    let target = resolve_target_with(config, setup.given.clone(), Some(&secrets))?;
    drop(secrets);

    if let Some(client_id) = config.missing_client_secret(&target) {
        anyhow::bail!(
            "No client secret for client '{}': set {CLIENT_SECRET_ENV}, or set it once and save \
             it with --save-config",
            display(client_id)
        );
    }
    Ok(target)
}

/// The config `--save-config` writes: the run's own, except that a profile
/// saved with a connection string given on this run (by flag or
/// environment, never one only stored) also records the namespace and event
/// hub that string names, so the profile names its destination.
///
/// # Errors
/// Fails when that string names no event hub and none was given, or names
/// another namespace than the one given.
fn config_to_save<'a>(setup: &TargetSetup<'a>) -> anyhow::Result<Cow<'a, ToolConfig>> {
    if setup.home.profile_name().is_none() {
        return Ok(Cow::Borrowed(setup.config));
    }
    with_typed_names(setup.config, setup.given)
}

pub(crate) fn saved_notice(
    given: &SensitiveValues,
    paths: &DataPaths,
    home: &ConfigHome,
) -> String {
    let saved = match home.profile_name() {
        Some(name) => format!("Saved profile {}", display(name)),
        None => "Saved config.json".to_owned(),
    };
    let secrets = match (
        given.connection_string.is_some(),
        given.client_secret.is_some(),
    ) {
        (true, _) => "the connection string",
        (false, true) => "the client secret",
        (false, false) => return saved,
    };
    format!(
        "{saved}; {secrets} is stored in {} (not encrypted)",
        display(paths.secrets_db().display().to_string())
    )
}

#[cfg(test)]
mod tests;
