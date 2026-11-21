//! The connection sheet's fields and their mapping to the CLI's config model.
//!
//! The form holds no validation rules of its own: names go through the engine's validators, and
//! required fields follow the config model (which fields each auth method reads).

use common_utils::string_utils::{SecretString, non_blank};
use rune::TextEdit;
use shared_eventhub_core::connection::{
    DEFAULT_CONSUMER_GROUP, validated_event_hub_name, validated_namespace,
};
use shared_eventhub_persistence::config::{
    AuthMethodConfig, CredentialKindConfig, SensitiveValues, StoredSecret, TargetConfig, ToolConfig,
};
use shared_eventhub_persistence::export::DiskWriteMode;

/// How the app authenticates to the event hub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthChoice {
    ConnectionString,
    Credential,
    KeyVault,
}

impl AuthChoice {
    pub const ALL: [AuthChoice; 3] = [Self::ConnectionString, Self::Credential, Self::KeyVault];
    pub const LABELS: [&'static str; 3] = ["Connection string", "Azure credential", "Key Vault"];

    pub fn index(self) -> usize {
        match self {
            Self::ConnectionString => 0,
            Self::Credential => 1,
            Self::KeyVault => 2,
        }
    }
}

/// The Entra ID credential, for credential and Key Vault auth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialChoice {
    DeveloperTools,
    ManagedIdentity,
    ClientSecret,
}

impl CredentialChoice {
    pub const ALL: [CredentialChoice; 3] = [
        Self::DeveloperTools,
        Self::ManagedIdentity,
        Self::ClientSecret,
    ];
    pub const LABELS: [&'static str; 3] = ["Developer tools", "Managed identity", "Client secret"];

    pub fn index(self) -> usize {
        match self {
            Self::DeveloperTools => 0,
            Self::ManagedIdentity => 1,
            Self::ClientSecret => 2,
        }
    }
}

/// The disk-write modes in the order the sheet shows them.
pub const DISK_WRITE_MODES: [DiskWriteMode; 3] = [
    DiskWriteMode::Off,
    DiskWriteMode::Immediate,
    DiskWriteMode::AtEnd,
];
pub const DISK_WRITE_LABELS: [&str; 3] = ["Off", "Immediate", "At end"];

pub fn disk_write_index(mode: DiskWriteMode) -> usize {
    match mode {
        DiskWriteMode::Off => 0,
        DiskWriteMode::Immediate => 1,
        DiskWriteMode::AtEnd => 2,
    }
}

/// A secret the form either takes as typed text or knows is in the data folder's secrets file.
/// A stored secret's bytes never enter the form: while `stored` is set the edit stays empty, and
/// the first edit switches the field to typed.
pub struct SecretSource {
    edit: TextEdit,
    stored: bool,
}

impl SecretSource {
    pub fn typed() -> Self {
        Self {
            edit: TextEdit::new("").secure(),
            stored: false,
        }
    }

    pub fn stored() -> Self {
        Self {
            stored: true,
            ..Self::typed()
        }
    }

    /// Whether the secret is the one in the secrets file.
    pub fn is_stored(&self) -> bool {
        self.stored
    }

    /// The typed field.
    pub fn edit(&self) -> &TextEdit {
        &self.edit
    }

    /// The typed field to edit; a stored secret becomes typed (the user replaces it).
    pub fn edit_mut(&mut self) -> &mut TextEdit {
        self.stored = false;
        &mut self.edit
    }

    fn is_empty(&self) -> bool {
        !self.stored && self.edit.text.trim().is_empty()
    }

    fn given(&self) -> Option<SecretString> {
        let text = self.edit.text.trim();
        (!self.stored && !text.is_empty()).then(|| SecretString::from(text.to_owned()))
    }

    fn clear(&mut self) {
        self.edit.clear();
    }

    fn hide(&mut self) {
        self.edit.set_revealed(false);
    }

    /// What a save of the current form does with this secret, given whether the config uses it.
    fn saving(&self, used: bool) -> SavedSecret {
        match (used, self.stored, self.given()) {
            (false, _, _) => SavedSecret::Unused,
            (true, true, _) | (true, false, None) => SavedSecret::Stored,
            (true, false, Some(value)) => SavedSecret::Typed(value),
        }
    }

    /// Matches the field to what a finished save left in the secrets file. Text typed after the
    /// save began stays typed.
    fn settle(&mut self, saved: &SavedSecret) {
        match saved {
            SavedSecret::Unused if self.stored => *self = Self::typed(),
            SavedSecret::Typed(value)
                if !self.stored && self.edit.text.trim() == value.expose() =>
            {
                *self = Self::stored();
            }
            SavedSecret::Unused | SavedSecret::Stored | SavedSecret::Typed(_) => {}
        }
    }
}

/// One of the two secrets the form can hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretKind {
    ConnectionString,
    ClientSecret,
}

/// What a save wrote for one secret.
#[derive(Clone, Debug)]
pub enum SavedSecret {
    /// The saved config does not use it, so the save removed any stored copy.
    Unused,
    /// Kept as already stored.
    Stored,
    /// Saved from this typed value.
    Typed(SecretString),
}

impl PartialEq for SavedSecret {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Unused, Self::Unused) | (Self::Stored, Self::Stored) => true,
            (Self::Typed(left), Self::Typed(right)) => left.expose() == right.expose(),
            (Self::Unused | Self::Stored | Self::Typed(_), _) => false,
        }
    }
}

impl Eq for SavedSecret {}

/// What a Save in flight writes for each secret, so its result can update the form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingSave {
    connection_string: SavedSecret,
    client_secret: SavedSecret,
}

/// What is wrong with each field, for the fields the chosen auth method shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldIssues {
    pub connection_string: Option<String>,
    pub namespace: Option<String>,
    pub event_hub: Option<String>,
    pub consumer_group: Option<String>,
    pub tenant_id: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub vault_url: Option<String>,
    pub secret_name: Option<String>,
    pub data_folder: Option<String>,
    pub received_msg_path: Option<String>,
}

impl FieldIssues {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Which of the namespace and event hub still hold, unedited, what the launch folder's
/// `config.json` gave.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecalledFields {
    pub namespace: bool,
    pub event_hub: bool,
}

impl RecalledFields {
    /// Whether neither field counts as recalled.
    pub fn is_empty(self) -> bool {
        !self.namespace && !self.event_hub
    }
}

/// The connection sheet's state. `Debug` prints no field text.
pub struct ConnectForm {
    pub auth: AuthChoice,
    pub credential: CredentialChoice,
    pub connection_string: SecretSource,
    pub client_secret: SecretSource,
    pub namespace: TextEdit,
    pub event_hub: TextEdit,
    pub consumer_group: TextEdit,
    pub tenant_id: TextEdit,
    pub client_id: TextEdit,
    pub vault_url: TextEdit,
    pub secret_name: TextEdit,
    /// Set only through the folder picker, which the state machine guards.
    pub data_folder: String,
    pub disk_write: DiskWriteMode,
    pub received_msg_path: TextEdit,
    /// Whether the namespace and event hub were filled from the launch folder's `config.json`.
    /// App code sets those two fields only with `TextEdit::new` or `replace_text`, so only the
    /// user's own edits make them count as typed (see [`Self::recalled_fields`]).
    pub(crate) names_from_launch_folder: bool,
    /// Settings the sheet does not show (capture and export settings, `content_only`), kept from
    /// a recalled config so saving does not drop them.
    base: ToolConfig,
}

impl std::fmt::Debug for ConnectForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectForm")
            .field("auth", &self.auth)
            .field("credential", &self.credential)
            .field("disk_write", &self.disk_write)
            .finish_non_exhaustive()
    }
}

impl Default for ConnectForm {
    fn default() -> Self {
        let base = ToolConfig::new(TargetConfig {
            namespace: None,
            event_hub: None,
            consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
            auth: AuthMethodConfig::Credential {
                credential: CredentialKindConfig::DeveloperTools,
            },
        });
        Self {
            auth: AuthChoice::ConnectionString,
            credential: CredentialChoice::DeveloperTools,
            connection_string: SecretSource::typed(),
            client_secret: SecretSource::typed(),
            namespace: TextEdit::new(""),
            event_hub: TextEdit::new(""),
            consumer_group: TextEdit::new(DEFAULT_CONSUMER_GROUP),
            tenant_id: TextEdit::new(""),
            client_id: TextEdit::new(""),
            vault_url: TextEdit::new(""),
            secret_name: TextEdit::new(""),
            data_folder: base.storage.base_data_folder.clone(),
            disk_write: base.storage.disk_write.mode,
            received_msg_path: TextEdit::new(base.storage.disk_write.received_msg_path.as_str()),
            names_from_launch_folder: false,
            base,
        }
    }
}

impl ConnectForm {
    /// The form for a recalled config, and the names of the fields it filled (names only,
    /// never values). Secrets the config uses are marked stored; their bytes stay on disk.
    pub fn from_config(config: &ToolConfig) -> (Self, Vec<&'static str>) {
        let mut form = Self {
            base: config.clone(),
            ..Self::default()
        };
        let mut loaded = Vec::new();
        let target = &config.target;
        if let Some(namespace) = &target.namespace {
            form.namespace = TextEdit::new(namespace.as_str());
            loaded.push("namespace");
        }
        if let Some(event_hub) = &target.event_hub {
            form.event_hub = TextEdit::new(event_hub.as_str());
            loaded.push("event hub");
        }
        form.consumer_group = TextEdit::new(target.consumer_group.as_str());
        loaded.push("consumer group");

        let credential = match &target.auth {
            AuthMethodConfig::ConnectionString { .. } => {
                form.auth = AuthChoice::ConnectionString;
                form.connection_string = SecretSource::stored();
                loaded.push("connection string (stored)");
                None
            }
            AuthMethodConfig::Credential { credential } => {
                form.auth = AuthChoice::Credential;
                Some(credential)
            }
            AuthMethodConfig::KeyVaultConnectionString {
                vault_url,
                secret_name,
                credential,
            } => {
                form.auth = AuthChoice::KeyVault;
                form.vault_url = TextEdit::new(vault_url.as_str());
                form.secret_name = TextEdit::new(secret_name.as_str());
                loaded.push("Key Vault reference");
                Some(credential)
            }
        };
        if let Some(credential) = credential {
            form.credential = match credential {
                CredentialKindConfig::DeveloperTools => CredentialChoice::DeveloperTools,
                CredentialKindConfig::ManagedIdentity => CredentialChoice::ManagedIdentity,
                CredentialKindConfig::ClientSecret {
                    tenant_id,
                    client_id,
                } => {
                    form.tenant_id = TextEdit::new(tenant_id.as_str());
                    form.client_id = TextEdit::new(client_id.as_str());
                    form.client_secret = SecretSource::stored();
                    loaded.push("client secret (stored)");
                    CredentialChoice::ClientSecret
                }
            };
            loaded.push("credential");
        }

        let storage = &config.storage;
        form.data_folder = storage.base_data_folder.clone();
        form.disk_write = storage.disk_write.mode;
        form.received_msg_path = TextEdit::new(storage.disk_write.received_msg_path.as_str());
        loaded.push("storage");
        (form, loaded)
    }

    /// Which secret holds typed text no save has stored yet, the connection string first;
    /// `None` when neither does.
    pub(crate) fn typed_secret(&self) -> Option<SecretKind> {
        if self.connection_string.given().is_some() {
            Some(SecretKind::ConnectionString)
        } else if self.client_secret.given().is_some() {
            Some(SecretKind::ClientSecret)
        } else {
            None
        }
    }

    /// The secret the chosen method uses that is the one in the secrets file: the connection
    /// string under connection-string auth, the client secret under a client-secret credential;
    /// `None` when that secret is typed or the method uses none.
    pub(crate) fn stored_secret(&self) -> Option<SecretKind> {
        if self.auth == AuthChoice::ConnectionString && self.connection_string.is_stored() {
            Some(SecretKind::ConnectionString)
        } else if self.uses_client_secret() && self.client_secret.is_stored() {
            Some(SecretKind::ClientSecret)
        } else {
            None
        }
    }

    /// The names that still hold what the launch folder's `config.json` gave: each counts while
    /// [`Self::names_from_launch_folder`] is set and the field is untouched (not empty, and no
    /// user edit since the app set it). Nothing compares or clears the text.
    pub(crate) fn recalled_fields(&self) -> RecalledFields {
        RecalledFields {
            namespace: self.names_from_launch_folder && untouched(&self.namespace),
            event_hub: self.names_from_launch_folder && untouched(&self.event_hub),
        }
    }

    /// Switches the auth method; typed secrets are hidden again.
    pub fn set_auth(&mut self, auth: AuthChoice) {
        self.auth = auth;
        self.hide_secrets();
    }

    /// Hides every typed secret (on sheet close, method switch, and window close).
    pub fn hide_secrets(&mut self) {
        self.connection_string.hide();
        self.client_secret.hide();
    }

    /// Wipes every typed secret; a stored secret stays marked stored.
    pub fn clear_secrets(&mut self) {
        self.connection_string.clear();
        self.client_secret.clear();
    }

    /// What saving the form as it is now writes for each secret.
    pub fn pending_save(&self) -> PendingSave {
        PendingSave {
            connection_string: self
                .connection_string
                .saving(self.auth == AuthChoice::ConnectionString),
            client_secret: self.client_secret.saving(self.uses_client_secret()),
        }
    }

    /// Updates the secret fields once `saved` landed: saved values become stored (and their text
    /// is wiped), and a stored secret the saved config dropped becomes an empty typed field.
    pub fn settle_after_save(&mut self, saved: &PendingSave) {
        self.connection_string.settle(&saved.connection_string);
        self.client_secret.settle(&saved.client_secret);
    }

    /// Forgets stored secrets: they belong to a data folder's secrets file, and the data folder
    /// changed.
    pub fn forget_stored_secrets(&mut self) {
        for secret in [&mut self.connection_string, &mut self.client_secret] {
            if secret.is_stored() {
                *secret = SecretSource::typed();
            }
        }
    }

    /// Whether the client secret fields apply to the chosen method.
    pub fn uses_client_secret(&self) -> bool {
        self.auth != AuthChoice::ConnectionString
            && self.credential == CredentialChoice::ClientSecret
    }

    /// What is wrong with the fields the chosen auth method shows.
    pub fn issues(&self) -> FieldIssues {
        let mut issues = FieldIssues {
            consumer_group: required(&self.consumer_group, "Enter a consumer group"),
            data_folder: self
                .data_folder
                .trim()
                .is_empty()
                .then(|| "Choose a data folder".to_owned()),
            ..FieldIssues::default()
        };
        if self.disk_write != DiskWriteMode::Off {
            issues.received_msg_path =
                required(&self.received_msg_path, "Enter a folder for message files");
        }

        let names_required = self.auth == AuthChoice::Credential;
        issues.namespace = name_issue(&self.namespace, names_required, validated_namespace);
        issues.event_hub = name_issue(&self.event_hub, names_required, validated_event_hub_name);

        match self.auth {
            AuthChoice::ConnectionString => {
                if self.connection_string.is_empty() {
                    issues.connection_string = Some("Enter the connection string".to_owned());
                }
            }
            AuthChoice::Credential => {}
            AuthChoice::KeyVault => {
                issues.vault_url = required(&self.vault_url, "Enter the vault URL");
                issues.secret_name = required(&self.secret_name, "Enter the secret name");
            }
        }
        if self.uses_client_secret() {
            issues.tenant_id = required(&self.tenant_id, "Enter the tenant id");
            issues.client_id = required(&self.client_id, "Enter the client id");
            if self.client_secret.is_empty() {
                issues.client_secret = Some("Enter the client secret".to_owned());
            }
        }
        issues
    }

    /// The config and typed secrets the form describes. Call only when [`issues`](Self::issues)
    /// is empty.
    pub fn to_config(&self) -> (ToolConfig, SensitiveValues) {
        let mut config = self.base.clone();
        config.target = TargetConfig {
            namespace: non_blank(&self.namespace.text),
            event_hub: non_blank(&self.event_hub.text),
            consumer_group: self.consumer_group.text.trim().to_owned(),
            auth: self.auth_config(),
        };
        config.storage.base_data_folder = self.data_folder.trim().to_owned();
        config.storage.disk_write.mode = self.disk_write;
        config.storage.disk_write.received_msg_path = self.received_msg_path.text.trim().to_owned();

        let sensitive = SensitiveValues {
            connection_string: match self.auth {
                AuthChoice::ConnectionString => self.connection_string.given(),
                AuthChoice::Credential | AuthChoice::KeyVault => None,
            },
            client_secret: if self.uses_client_secret() {
                self.client_secret.given()
            } else {
                None
            },
        };
        (config, sensitive)
    }

    fn auth_config(&self) -> AuthMethodConfig {
        let credential = match self.credential {
            CredentialChoice::DeveloperTools => CredentialKindConfig::DeveloperTools,
            CredentialChoice::ManagedIdentity => CredentialKindConfig::ManagedIdentity,
            CredentialChoice::ClientSecret => CredentialKindConfig::ClientSecret {
                tenant_id: self.tenant_id.text.trim().to_owned(),
                client_id: self.client_id.text.trim().to_owned(),
            },
        };
        match self.auth {
            AuthChoice::ConnectionString => AuthMethodConfig::ConnectionString {
                stored: StoredSecret::LocalDb,
            },
            AuthChoice::Credential => AuthMethodConfig::Credential { credential },
            AuthChoice::KeyVault => AuthMethodConfig::KeyVaultConnectionString {
                vault_url: self.vault_url.text.trim().to_owned(),
                secret_name: self.secret_name.text.trim().to_owned(),
                credential,
            },
        }
    }
}

/// Whether `edit` holds what the app last set with `TextEdit::new` or `replace_text`, with no
/// user edit since: Rune's pristine flag, and no undo or redo step. Rune records every edit of a
/// plain field as a history step, but its plain-text edits leave the pristine flag set, so the
/// flag alone misses typing, deleting, and pasting there.
fn untouched(edit: &TextEdit) -> bool {
    edit.is_pristine() && edit.history_len() == (0, 0)
}

fn required(edit: &TextEdit, message: &str) -> Option<String> {
    edit.text.trim().is_empty().then(|| message.to_owned())
}

/// An empty optional name is fine; anything typed must pass the engine's validator.
fn name_issue(
    edit: &TextEdit,
    required: bool,
    validate: fn(&str) -> anyhow::Result<String>,
) -> Option<String> {
    let text = edit.text.trim();
    if text.is_empty() {
        return required.then(|| "Required for this sign-in method".to_owned());
    }
    validate(text).err().map(|error| format!("{error:#}"))
}

#[cfg(test)]
mod tests;
