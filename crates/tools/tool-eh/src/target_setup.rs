//! Turning a run's merged configuration into the event hub target it
//! connects to, saving `config.json` first when asked.

use crate::models::CLIENT_SECRET_ENV;
use crate::progress::{Renderer, display};
use shared_eventhub_core::connection::EventHubTarget;
use shared_eventhub_persistence::config::{
    SensitiveValues, ToolConfig, needs_stored_secret, resolve_target_with, save_config,
};
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::secrets::SecretsStore;
use std::io::Write;
use std::path::Path;

/// What [`prepare_target`] works from.
pub(crate) struct TargetSetup<'a> {
    pub config: &'a ToolConfig,
    /// Secrets given by flag or environment.
    pub given: &'a SensitiveValues,
    pub paths: &'a DataPaths,
    pub save_config: bool,
    /// Where `config.json` is saved.
    pub working_dir: &'a Path,
}

/// Builds the event hub target, reading the secrets file only when a secret
/// the auth method uses was not given, and saves the config first when
/// `save_config` is set.
///
/// # Errors
/// Fails when the secrets file cannot be opened or read, when saving fails,
/// when a stored connection string is missing, or when a client-secret
/// credential has no secret.
pub(crate) fn prepare_target<O: Write, E: Write>(
    setup: &TargetSetup<'_>,
    renderer: &mut Renderer<O, E>,
) -> anyhow::Result<EventHubTarget> {
    let config = setup.config;
    if !setup.save_config && !needs_stored_secret(config, setup.given) {
        return resolve_target_with(config, setup.given.clone(), None);
    }

    let secrets = SecretsStore::open(setup.paths)?;
    if setup.save_config {
        save_config(setup.working_dir, config, &secrets, setup.given)?;
        renderer.notice(&saved_notice(setup.given, setup.paths));
    }
    let target = resolve_target_with(config, setup.given.clone(), Some(&secrets))?;
    drop(secrets);

    if let (Some(client_id), None) = (config.client_secret_id(), &target.client_secret) {
        anyhow::bail!(
            "No client secret for client '{}': set {CLIENT_SECRET_ENV}, or set it once and save \
             it with --save-config",
            display(client_id)
        );
    }
    Ok(target)
}

pub(crate) fn saved_notice(given: &SensitiveValues, paths: &DataPaths) -> String {
    let secrets = match (
        given.connection_string.is_some(),
        given.client_secret.is_some(),
    ) {
        (true, _) => "the connection string",
        (false, true) => "the client secret",
        (false, false) => return "Saved config.json".to_owned(),
    };
    format!(
        "Saved config.json; {secrets} is stored in {} (not encrypted)",
        display(paths.secrets_db().display().to_string())
    )
}

#[cfg(test)]
mod tests;
