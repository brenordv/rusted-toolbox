//! The demo's profile workspace: two profiles that load and one that can't, written under the
//! demo's temporary root, and the seeded sheet states over them.

use super::DEMO_CONNECTION_STRING;
use crate::services::{listed_profiles, open_secrets};
use crate::state::{AppState, Effect, Input, SheetPurpose};
use anyhow::Context;
use common_utils::string_utils::SecretString;
use eh_persistence::config::{
    AuthMethodConfig, CONFIG_FILE_NAME, CredentialKindConfig, SensitiveValues, StoredSecret,
    TargetConfig, ToolConfig, save_config,
};
use eh_persistence::paths::DataPaths;
use eh_persistence::workspace::Workspace;
use std::path::Path;
use tokio::runtime::Handle;
use tracing::debug;

/// The workspace folder inside the demo's root.
const WORKSPACE_FOLDER: &str = "workspace";
/// A profile with a stored connection string.
pub const ORDERS_DEV: &str = "orders-dev";
/// A profile that signs in with Entra ID.
pub const BILLING: &str = "billing";
/// A profile folder whose `config.json` is marked eh's but is not valid.
const SCRATCH: &str = "scratch";
/// What `scratch` holds: eh's marker and no target.
const BROKEN_CONFIG: &str = "{\"tool\": \"eh\", \"schema_version\": 1}\n";
/// The failure `profiles-failed` shows for `billing`, worded as the loader words one.
const LOAD_FAILURE: &str = "the config.json is an eh config, but it is not valid: missing field \
    `target` at line 1 column 34";

/// The demo's workspace: `<root>/workspace`, empty until [`seed_profiles`] fills it.
pub fn workspace(root: &Path) -> Workspace {
    Workspace::at(root.join(WORKSPACE_FOLDER))
}

/// Writes the demo's profiles: `orders-dev` (a stored connection string), `billing` (Entra
/// ID), and `scratch`, which can't load. Each data folder stays inside its profile folder.
///
/// # Errors
/// Fails when a folder, a secrets file, or a `config.json` cannot be written.
pub fn seed_profiles(root: &Path) -> anyhow::Result<()> {
    let workspace = workspace(root);
    let orders = ToolConfig::new(TargetConfig {
        namespace: None,
        event_hub: None,
        consumer_group: "$Default".to_owned(),
        auth: AuthMethodConfig::ConnectionString {
            stored: StoredSecret::LocalDb,
        },
    });
    let given = SensitiveValues {
        connection_string: Some(SecretString::from(DEMO_CONNECTION_STRING.to_owned())),
        client_secret: None,
    };
    save_profile(workspace.root(), ORDERS_DEV, &orders, &given)?;
    let billing = ToolConfig::new(TargetConfig {
        namespace: Some("billing.servicebus.windows.net".to_owned()),
        event_hub: Some("invoices".to_owned()),
        consumer_group: "$Default".to_owned(),
        auth: AuthMethodConfig::Credential {
            credential: CredentialKindConfig::DeveloperTools,
        },
    });
    save_profile(
        workspace.root(),
        BILLING,
        &billing,
        &SensitiveValues::default(),
    )?;
    let scratch = workspace.root().join(SCRATCH);
    std::fs::create_dir_all(&scratch).context("Failed to create the demo's broken profile")?;
    std::fs::write(scratch.join(CONFIG_FILE_NAME), BROKEN_CONFIG)
        .context("Failed to write the demo's broken profile")?;
    debug!("seeded the demo profiles");
    Ok(())
}

fn save_profile(
    root: &Path,
    name: &str,
    config: &ToolConfig,
    given: &SensitiveValues,
) -> anyhow::Result<()> {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create profile {name}"))?;
    let paths = DataPaths::new(dir.join(&config.storage.base_data_folder));
    open_secrets(&paths, |secrets| save_config(&dir, config, secrets, given))
        .with_context(|| format!("Failed to save profile {name}"))
}

/// Opens the sheet on `state` and answers its listing on the spot (a seeded state runs no
/// services), so the picker shows the workspace.
pub fn open_listed(state: &mut AppState, rt: &Handle) {
    let out = state.apply(Input::OpenSheet(SheetPurpose::Connect));
    for effect in out.effects {
        if let Effect::ListProfiles { request, workspace } = effect {
            let listed = rt.block_on(listed_profiles(request, workspace));
            state.apply(listed);
        }
    }
}

/// `profiles-locked`: a connect cancelled while preparing, whose prepare never answers, holds
/// the data folder while the sheet shows the picker.
pub fn locked(state: &mut AppState, rt: &Handle) {
    drop(state.apply(Input::ConnectPressed));
    drop(state.apply(Input::DisconnectPressed));
    open_listed(state, rt);
}

/// `profiles-failed`: a pick of `billing` whose load failed, its note under the picker.
pub fn failed(state: &mut AppState, rt: &Handle) {
    open_listed(state, rt);
    let Some(billing) = state.home_for_label(BILLING) else {
        debug!("billing is not listed; the demo shows the plain picker");
        return;
    };
    let out = state.apply(Input::ProfilePicked(billing));
    for effect in out.effects {
        if let Effect::LoadProfile { request, .. } = effect {
            state.apply(Input::ProfileLoaded {
                request,
                result: Err(LOAD_FAILURE.to_owned()),
            });
        }
    }
}
