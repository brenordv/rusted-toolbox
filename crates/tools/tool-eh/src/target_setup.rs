//! Turning a run's merged configuration into the event hub target it
//! connects to, saving `config.json` first when asked.

use crate::models::CLIENT_SECRET_ENV;
use crate::progress::{Renderer, display};
use shared_eventhub_core::connection::{EventHubTarget, target_names};
use shared_eventhub_persistence::config::{
    AuthMethodConfig, SensitiveValues, ToolConfig, needs_stored_secret, resolve_target_with,
    save_config,
};
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::secrets::SecretsStore;
use shared_eventhub_persistence::workspace::ConfigHome;
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
    let config = setup.config;
    let string_given = match config.target.auth {
        AuthMethodConfig::ConnectionString { .. } => setup.given.connection_string.is_some(),
        AuthMethodConfig::Credential { .. } | AuthMethodConfig::KeyVaultConnectionString { .. } => {
            false
        }
    };
    let names_missing = config.target.namespace.is_none() || config.target.event_hub.is_none();
    if setup.home.profile_name().is_none() || !string_given || !names_missing {
        return Ok(Cow::Borrowed(config));
    }

    let target = resolve_target_with(config, setup.given.clone(), None)?;
    let Some(names) = target_names(&target) else {
        return Ok(Cow::Borrowed(config));
    };
    let (namespace, event_hub) = names?;
    let mut named = config.clone();
    named.target.namespace.get_or_insert(namespace);
    named.target.event_hub.get_or_insert(event_hub);
    Ok(Cow::Owned(named))
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
