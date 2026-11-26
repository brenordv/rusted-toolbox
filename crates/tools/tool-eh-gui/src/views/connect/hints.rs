//! The connection sheet's hints: a muted line under each sign-in field saying what it is, ending
//! with an example value where one helps, and a line under each picker saying what the picked
//! option does. An example ends its line with no period after it, since a period after a host
//! name or a URL reads as part of the value.

use crate::form::{AuthChoice, CredentialChoice};

/// Under the connection string field. The example follows on its own line,
/// [`CONNECTION_STRING_EXAMPLE`].
pub const CONNECTION_STRING: &str = "The event hub's connection string. In the Azure portal, on \
    the event hub or its namespace: Shared access policies, a policy with only the rights you \
    need (Listen to read, Send to send), then Connection string-primary key. Example:";
/// The connection string's example value, on its own line under [`CONNECTION_STRING`]. The key
/// is three dots, not a key.
pub const CONNECTION_STRING_EXAMPLE: &str = "Endpoint=sb://my-ns.servicebus.windows.net/;\
    SharedAccessKeyName=my-policy;SharedAccessKey=...;EntityPath=orders";
/// Under the tenant id field.
pub const TENANT_ID: &str = "The app registration's Directory (tenant) ID, on its Overview page.";
/// Under the client id field.
pub const CLIENT_ID: &str = "The app registration's Application (client) ID, on its Overview page.";
/// Under the client secret field.
pub const CLIENT_SECRET: &str =
    "A client secret's Value, from the app registration's Certificates & secrets.";
/// Under the vault URL field.
pub const VAULT_URL: &str = "The vault's address. Example: https://my-vault.vault.azure.net";
/// Under the secret name field.
pub const SECRET_NAME: &str =
    "The name of the secret that holds the connection string. Example: eh-orders";
/// Under the consumer group field.
pub const CONSUMER_GROUP: &str = "The consumer group to read with; it must already exist. Every \
    event hub has $Default. Example: eh-local";

/// Under the Sign-in method picker: what `auth` does.
pub fn sign_in(auth: AuthChoice) -> &'static str {
    match auth {
        AuthChoice::ConnectionString => {
            "Signs in with the shared access key in an Event Hubs connection string."
        }
        AuthChoice::Credential => {
            "Signs in with Microsoft Entra ID. Needs the namespace and the event hub."
        }
        AuthChoice::KeyVault => {
            "Reads the connection string from a Key Vault secret, signing in to the vault with \
             the Azure credential below."
        }
    }
}

/// Under the Azure credential picker: what `credential` uses.
pub fn credential(credential: CredentialChoice) -> &'static str {
    match credential {
        CredentialChoice::DeveloperTools => {
            "Uses the account signed in to the Azure CLI or the Azure Developer CLI."
        }
        CredentialChoice::ManagedIdentity => {
            "Uses the system-assigned managed identity of the Azure resource this runs on."
        }
        CredentialChoice::ClientSecret => {
            "Uses an app registration: its tenant id, client id, and a client secret."
        }
    }
}

/// Under the namespace field under `auth`. Credential auth needs the name; the other two take
/// it from the connection string when it is left empty and refuse a typed one that differs.
pub fn namespace(auth: AuthChoice) -> &'static str {
    match auth {
        AuthChoice::ConnectionString => {
            "The namespace's host name. Leave empty to use the connection string's; if filled, \
             it must match. Example: my-ns.servicebus.windows.net"
        }
        AuthChoice::Credential => {
            "The namespace's host name. Example: my-ns.servicebus.windows.net"
        }
        AuthChoice::KeyVault => {
            "The namespace's host name. Leave empty to use the one in the secret's connection \
             string; if filled, it must match. Example: my-ns.servicebus.windows.net"
        }
    }
}

/// Under the event hub field under `auth`. Credential auth needs the name; the other two need
/// it only for a connection string with no `EntityPath`, and refuse a typed one that differs.
pub fn event_hub(auth: AuthChoice) -> &'static str {
    match auth {
        AuthChoice::ConnectionString => {
            "The event hub's name. Needed when the connection string has no EntityPath; \
             otherwise leave empty or type the same name. Example: orders"
        }
        AuthChoice::Credential => "The event hub's name. Example: orders",
        AuthChoice::KeyVault => {
            "The event hub's name. Needed when the secret's connection string has no \
             EntityPath; otherwise leave empty or type the same name. Example: orders"
        }
    }
}
