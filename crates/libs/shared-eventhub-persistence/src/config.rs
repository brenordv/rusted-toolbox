//! `config.json`: a tool run's settings saved in the working directory and
//! recalled on the next run. The file never holds a secret: connection
//! strings and client secrets go to the data folder's secrets file, and the
//! config only says where to find them.

use crate::export::{DiskWriteMode, ExportFormat};
use crate::secrets::{CONNECTION_STRING_SECRET, SecretsStore, client_secret_name};
use anyhow::Context;
use common_file_utils::atomic_write::write_via_temp;
use common_utils::string_utils::SecretString;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use shared_azure::auth::{AuthMethod, CredentialKind};
use shared_eventhub_core::connection::{DEFAULT_CONSUMER_GROUP, EventHubTarget};
use shared_eventhub_core::filters::CaptureFilters;
use std::collections::BTreeSet;
use std::path::Path;
use tracing::info;

/// The config file's name, in the working directory.
pub const CONFIG_FILE_NAME: &str = "config.json";
/// The `tool` value that marks a `config.json` as this tool's.
pub const TOOL_MARKER: &str = "eh";
/// The config schema version this build reads and writes.
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

const STORED_IN_SECRETS_FILE: &str = "local-db";

/// A saved run configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolConfig {
    /// Always [`TOOL_MARKER`]; a `config.json` without it belongs to another
    /// tool.
    pub tool: String,
    pub schema_version: u32,
    pub target: TargetConfig,
    #[serde(default)]
    pub capture: CaptureConfig,
    #[serde(default)]
    pub storage: StorageConfig,
}

/// The event hub to read and how to authenticate to it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetConfig {
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub event_hub: Option<String>,
    #[serde(default = "default_consumer_group")]
    pub consumer_group: String,
    pub auth: AuthMethodConfig,
}

/// How to authenticate. No variant can hold a secret value, so no config can
/// leak one through serialization.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "kebab-case", deny_unknown_fields)]
pub enum AuthMethodConfig {
    /// The connection string is in the secrets file.
    ConnectionString {
        stored: StoredSecret,
    },
    Credential {
        credential: CredentialKindConfig,
    },
    /// The connection string is fetched from Key Vault on every run; only the
    /// reference is saved.
    KeyVaultConnectionString {
        vault_url: String,
        secret_name: String,
        credential: CredentialKindConfig,
    },
}

/// The Entra ID credential to use. A client secret, when needed, is in the
/// secrets file under the client id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum CredentialKindConfig {
    DeveloperTools,
    ManagedIdentity,
    ClientSecret {
        tenant_id: String,
        client_id: String,
    },
}

/// Marks a secret as kept in the data folder's secrets file. Serialized as
/// `"local-db"`, the only value it accepts; the rejection never quotes what
/// was found, because a mistake here is usually a pasted secret.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StoredSecret {
    #[default]
    LocalDb,
}

impl Serialize for StoredSecret {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::LocalDb => serializer.serialize_str(STORED_IN_SECRETS_FILE),
        }
    }
}

impl<'de> Deserialize<'de> for StoredSecret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value == STORED_IN_SECRETS_FILE {
            Ok(Self::LocalDb)
        } else {
            Err(D::Error::custom(
                "auth.stored must be `local-db`; config.json never holds the secret itself",
            ))
        }
    }
}

/// Capture settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct CaptureConfig {
    /// Partitions to read; `None` reads all of them.
    pub partitions: Option<Vec<String>>,
    pub ignore_checkpoint: bool,
    /// Receiver prefetch credits; `None` keeps the SDK default.
    pub prefetch: Option<u32>,
    pub feedback_interval_secs: u64,
    pub filters: CaptureFilters,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            partitions: None,
            ignore_checkpoint: false,
            prefetch: None,
            feedback_interval_secs: 1,
            filters: CaptureFilters::default(),
        }
    }
}

/// Where data lives and how it is written out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct StorageConfig {
    pub base_data_folder: String,
    pub disk_write: DiskWriteSettings,
    pub export: ExportSettings,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            base_data_folder: ".eh-data".to_owned(),
            disk_write: DiskWriteSettings::default(),
            export: ExportSettings::default(),
        }
    }
}

/// The disk-write mode and where its files go (relative to the data folder
/// unless absolute).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct DiskWriteSettings {
    pub mode: DiskWriteMode,
    pub received_msg_path: String,
    pub content_only: bool,
}

impl Default for DiskWriteSettings {
    fn default() -> Self {
        Self {
            mode: DiskWriteMode::Off,
            received_msg_path: "inbound".to_owned(),
            content_only: false,
        }
    }
}

/// Export settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ExportSettings {
    pub format: ExportFormat,
    pub include_metadata: bool,
    pub condense_output: bool,
    pub use_local_time: bool,
    pub export_folder: String,
    pub skip_already_exported: bool,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            format: ExportFormat::Txt,
            include_metadata: true,
            condense_output: false,
            use_local_time: false,
            export_folder: "exports".to_owned(),
            skip_already_exported: true,
        }
    }
}

fn default_consumer_group() -> String {
    DEFAULT_CONSUMER_GROUP.to_owned()
}

impl ToolConfig {
    /// A config for `target` with every other setting at its default.
    pub fn new(target: TargetConfig) -> Self {
        Self {
            tool: TOOL_MARKER.to_owned(),
            schema_version: CONFIG_SCHEMA_VERSION,
            target,
            capture: CaptureConfig::default(),
            storage: StorageConfig::default(),
        }
    }

    /// The client id whose secret the config needs, from the event hub
    /// credential or the Key Vault credential.
    fn client_secret_id(&self) -> Option<&str> {
        let kind = match &self.target.auth {
            AuthMethodConfig::ConnectionString { .. } => return None,
            AuthMethodConfig::Credential { credential }
            | AuthMethodConfig::KeyVaultConnectionString { credential, .. } => credential,
        };
        match kind {
            CredentialKindConfig::ClientSecret { client_id, .. } => Some(client_id),
            CredentialKindConfig::DeveloperTools | CredentialKindConfig::ManagedIdentity => None,
        }
    }

    /// The secret names this config reads from the secrets file.
    fn referenced_secrets(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        match self.target.auth {
            AuthMethodConfig::ConnectionString { .. } => {
                names.insert(CONNECTION_STRING_SECRET.to_owned());
            }
            AuthMethodConfig::Credential { .. }
            | AuthMethodConfig::KeyVaultConnectionString { .. } => {}
        }
        if let Some(client_id) = self.client_secret_id() {
            names.insert(client_secret_name(client_id));
        }
        names
    }
}

impl From<&CredentialKindConfig> for CredentialKind {
    fn from(kind: &CredentialKindConfig) -> Self {
        match kind {
            CredentialKindConfig::DeveloperTools => Self::DeveloperTools,
            CredentialKindConfig::ManagedIdentity => Self::ManagedIdentity,
            CredentialKindConfig::ClientSecret {
                tenant_id,
                client_id,
            } => Self::ClientSecret {
                tenant_id: tenant_id.clone(),
                client_id: client_id.clone(),
            },
        }
    }
}

/// Secret values entered for this run, to be kept in the secrets file.
#[derive(Clone, Debug, Default)]
pub struct SensitiveValues {
    pub connection_string: Option<SecretString>,
    pub client_secret: Option<SecretString>,
}

/// What [`load_config`] found.
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigLoad {
    /// No `config.json` in the directory.
    Missing,
    /// A `config.json` that is not this tool's (no `"tool": "eh"`, or not a
    /// JSON object at all); it is ignored.
    Foreign,
    Loaded(Box<ToolConfig>),
}

/// Reads `config.json` from `dir`.
///
/// # Errors
/// Fails when the file cannot be read, or when it is marked as this tool's
/// but does not match the config schema.
pub async fn load_config(dir: &Path) -> anyhow::Result<ConfigLoad> {
    let path = dir.join(CONFIG_FILE_NAME);
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ConfigLoad::Missing);
        }
        Err(error) => {
            return Err(
                anyhow::Error::new(error).context(format!("Failed to read '{}'", path.display()))
            );
        }
    };
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{FEFF}');
    if !is_ours(text) {
        info!(path = %path.display(), "Ignoring a config.json that is not an eh config");
        return Ok(ConfigLoad::Foreign);
    }

    let config: ToolConfig = serde_json::from_str(text).map_err(|error| {
        anyhow::anyhow!(
            "'{}' is an eh config, but it is not valid: {}",
            path.display(),
            redact_quoted(&error.to_string())
        )
    })?;
    if config.schema_version != CONFIG_SCHEMA_VERSION {
        anyhow::bail!(
            "'{}' has config schema version {}; this build reads version {CONFIG_SCHEMA_VERSION}",
            path.display(),
            config.schema_version
        );
    }
    info!(
        path = %path.display(),
        namespace = config.target.namespace.as_deref().unwrap_or(""),
        event_hub = config.target.event_hub.as_deref().unwrap_or(""),
        auth = auth_method_name(&config.target.auth),
        "Loaded config.json"
    );
    Ok(ConfigLoad::Loaded(Box::new(config)))
}

/// Whether `text` is a JSON object marked as this tool's config. Text that is
/// not JSON at all (another tool's JSONC, a truncated or UTF-16 file) cannot
/// be this tool's config. A leading UTF-8 BOM is ignored.
fn is_ours(text: &str) -> bool {
    let parsed = serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{FEFF}'));
    let Ok(document) = parsed else {
        return false;
    };

    document.get("tool").and_then(serde_json::Value::as_str) == Some(TOOL_MARKER)
}

/// Replaces every double-quoted run in a serde message with `"<redacted>"`.
/// serde quotes the offending string value in type errors, and in a config a
/// misplaced string is most often a pasted secret.
fn redact_quoted(message: &str) -> String {
    message
        .split('"')
        .enumerate()
        .map(|(index, part)| if index % 2 == 0 { part } else { "<redacted>" })
        .collect::<Vec<_>>()
        .join("\"")
}

fn auth_method_name(auth: &AuthMethodConfig) -> &'static str {
    match auth {
        AuthMethodConfig::ConnectionString { .. } => "connection-string",
        AuthMethodConfig::Credential { .. } => "credential",
        AuthMethodConfig::KeyVaultConnectionString { .. } => "key-vault-connection-string",
    }
}

/// Saves `config` as `dir/config.json` and its secrets to `secrets`.
///
/// Secrets are written first, so the config never points at a missing one;
/// once the config is written, secrets it no longer references are removed.
///
/// # Errors
/// Fails without writing anything when `config` is not marked as this tool's,
/// when an existing `config.json` belongs to another tool or cannot be
/// parsed, when a Key Vault config comes with a connection string (the
/// fetched value must never reach disk), when connection-string auth has no
/// connection string given or stored, or when a client secret is given but no
/// client-secret credential uses it. Also fails when writing fails.
pub fn save_config(
    dir: &Path,
    config: &ToolConfig,
    secrets: &SecretsStore,
    sensitive: &SensitiveValues,
) -> anyhow::Result<()> {
    let path = dir.join(CONFIG_FILE_NAME);
    check_savable(&path, config, secrets, sensitive)?;

    if let (AuthMethodConfig::ConnectionString { .. }, Some(connection_string)) =
        (&config.target.auth, &sensitive.connection_string)
    {
        secrets.put(CONNECTION_STRING_SECRET, connection_string)?;
    }
    if let (Some(client_id), Some(client_secret)) =
        (config.client_secret_id(), &sensitive.client_secret)
    {
        secrets.put(&client_secret_name(client_id), client_secret)?;
    }

    let mut document =
        serde_json::to_string_pretty(config).context("Failed to serialize the config")?;
    document.push('\n');
    write_via_temp(&path, |temp| {
        std::fs::write(temp, &document).context("Failed to write the config")
    })
    .with_context(|| format!("Failed to save '{}'", path.display()))?;

    let referenced = config.referenced_secrets();
    for name in secrets.names()? {
        if !referenced.contains(&name) {
            secrets.remove(&name)?;
        }
    }
    info!(
        path = %path.display(),
        auth = auth_method_name(&config.target.auth),
        "Saved config.json"
    );
    Ok(())
}

fn check_savable(
    path: &Path,
    config: &ToolConfig,
    secrets: &SecretsStore,
    sensitive: &SensitiveValues,
) -> anyhow::Result<()> {
    if config.tool != TOOL_MARKER || config.schema_version != CONFIG_SCHEMA_VERSION {
        anyhow::bail!(
            "Refusing to save a config without \"tool\": \"{TOOL_MARKER}\" and schema version \
             {CONFIG_SCHEMA_VERSION}"
        );
    }
    if path.exists() {
        let existing = std::fs::read(path)
            .with_context(|| format!("Failed to read the existing '{}'", path.display()))?;
        if !is_ours(&String::from_utf8_lossy(&existing)) {
            anyhow::bail!(
                "'{}' belongs to another tool (it has no \"tool\": \"{TOOL_MARKER}\"); move or \
                 rename it before saving",
                path.display()
            );
        }
    }

    match &config.target.auth {
        AuthMethodConfig::KeyVaultConnectionString { .. }
            if sensitive.connection_string.is_some() =>
        {
            anyhow::bail!(
                "A Key Vault config keeps only the vault reference; the connection string it \
                 fetches is never saved"
            )
        }
        AuthMethodConfig::ConnectionString { .. }
            if sensitive.connection_string.is_none()
                && secrets.get(CONNECTION_STRING_SECRET)?.is_none() =>
        {
            anyhow::bail!(
                "There is no connection string to save: none was given and none is stored"
            )
        }
        AuthMethodConfig::ConnectionString { .. }
        | AuthMethodConfig::Credential { .. }
        | AuthMethodConfig::KeyVaultConnectionString { .. } => {}
    }
    if sensitive.client_secret.is_some() && config.client_secret_id().is_none() {
        anyhow::bail!("A client secret was given, but the config uses no client-secret credential");
    }
    Ok(())
}

/// Rebuilds the event hub target `config` describes, with its secrets read
/// from `secrets`.
///
/// # Errors
/// Fails when the config uses a stored connection string and the secrets
/// file does not hold one, or when the secrets file cannot be read.
pub fn resolve_target(
    config: &ToolConfig,
    secrets: &SecretsStore,
) -> anyhow::Result<EventHubTarget> {
    let auth = match &config.target.auth {
        AuthMethodConfig::ConnectionString { .. } => AuthMethod::ConnectionString {
            connection_string: secrets.get(CONNECTION_STRING_SECRET)?.context(
                "config.json expects the connection string in the data folder's secrets file, \
                 but it is not there; save the config again with the connection string",
            )?,
        },
        AuthMethodConfig::Credential { credential } => AuthMethod::Credential {
            kind: credential.into(),
        },
        AuthMethodConfig::KeyVaultConnectionString {
            vault_url,
            secret_name,
            credential,
        } => AuthMethod::KeyVaultConnectionString {
            vault_url: vault_url.clone(),
            secret_name: secret_name.clone(),
            credential: credential.into(),
        },
    };
    let client_secret = match config.client_secret_id() {
        Some(client_id) => secrets.get(&client_secret_name(client_id))?,
        None => None,
    };
    info!(
        auth = auth_method_name(&config.target.auth),
        client_secret_stored = client_secret.is_some(),
        "Resolved the saved target"
    );

    Ok(EventHubTarget {
        auth,
        client_secret,
        fully_qualified_namespace: config.target.namespace.clone(),
        event_hub_name: config.target.event_hub.clone(),
        consumer_group: config.target.consumer_group.clone(),
    })
}

#[cfg(test)]
mod tests;
