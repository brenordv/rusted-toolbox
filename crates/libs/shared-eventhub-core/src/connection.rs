//! Describing an Event Hubs target and opening a validated consumer client
//! for it.

use crate::models::system_time_to_utc;
use anyhow::Context;
use azure_core::credentials::TokenCredential;
use azure_messaging_eventhubs::builders::ConsumerClientBuilder;
use azure_messaging_eventhubs::{ConnectionString, ConsumerClient};
use chrono::{DateTime, Utc};
use common_utils::string_utils::SecretString;
use shared_azure::auth::{AuthMethod, CredentialKind, build_credential};
use shared_azure::host::host_has_allowed_suffix;
use shared_azure::key_vault::fetch_secret;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{info, warn};

/// The consumer group every event hub has.
pub const DEFAULT_CONSUMER_GROUP: &str = "$Default";

/// DNS suffixes an Event Hubs namespace host must sit under, per Azure cloud:
/// public, China (21Vianet), and US Government. The domains are listed in the
/// Service Bus REST reference (<https://learn.microsoft.com/en-us/rest/api/servicebus/>)
/// and, for Azure Government, the Event Hubs row of
/// <https://learn.microsoft.com/en-us/azure/azure-government/compare-azure-government-global-azure>.
/// Entra ID tokens for the Event Hubs audience are presented to this host, so
/// it is an allowlist, not a hint.
pub const EVENT_HUBS_ALLOWED_HOST_SUFFIXES: [&str; 3] = [
    "servicebus.windows.net",
    "servicebus.chinacloudapi.cn",
    "servicebus.usgovcloudapi.net",
];

const MAX_EVENT_HUB_NAME_LENGTH: usize = 256;

/// Everything needed to reach one event hub, and the consumer group to read
/// it through (`consumer_group` applies to consumers only).
#[derive(Clone, Debug)]
pub struct EventHubTarget {
    pub auth: AuthMethod,
    /// Secret for [`CredentialKind::ClientSecret`], wherever that kind
    /// appears in `auth` (the event hub credential or the Key Vault
    /// credential). Ignored by the other kinds.
    pub client_secret: Option<SecretString>,
    /// Required for credential auth; for connection-string auth it is taken
    /// from the connection string's `Endpoint`, and a value given here must
    /// match it.
    pub fully_qualified_namespace: Option<String>,
    /// Required unless the connection string carries an `EntityPath`; when
    /// both are given they must match.
    pub event_hub_name: Option<String>,
    pub consumer_group: String,
}

/// A validated event hub: the lowercased namespace host, the hub name, and
/// when the service created the hub.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventHubIdentity {
    pub fully_qualified_namespace: String,
    pub event_hub_name: String,
    /// When the service created the event hub; a hub deleted and recreated
    /// under the same name has a new one. `None` when the service did not
    /// report it.
    pub created_on: Option<DateTime<Utc>>,
}

/// An open consumer client whose credential has been exercised once (the
/// event hub properties were read with it).
pub struct EventHubConnection {
    pub client: ConsumerClient,
    pub fully_qualified_namespace: String,
    pub event_hub_name: String,
    /// When the service created the event hub, as it reported it.
    pub created_on: Option<DateTime<Utc>>,
    /// The event hub's partitions, in the order the service listed them.
    pub partition_ids: Vec<String>,
}

/// Where the client's authentication comes from once the target is resolved.
pub(crate) enum ClientAuth {
    ConnectionString(SecretString),
    Credential(CredentialKind),
}

/// A target that passed validation: names are checked and the namespace host
/// is on the allowlist.
pub(crate) struct ResolvedTarget {
    pub(crate) fully_qualified_namespace: String,
    pub(crate) event_hub_name: String,
    pub(crate) auth: ClientAuth,
}

/// An SDK client builder that can open its client either way the resolved
/// auth allows.
pub(crate) trait OpenClient {
    type Client;

    fn with_connection_string(
        self,
        connection_string: &str,
        event_hub_name: &str,
    ) -> impl Future<Output = azure_messaging_eventhubs::Result<Self::Client>> + Send;

    fn with_credential(
        self,
        fully_qualified_namespace: &str,
        event_hub_name: &str,
        credential: Arc<dyn TokenCredential>,
    ) -> impl Future<Output = azure_messaging_eventhubs::Result<Self::Client>> + Send;
}

impl OpenClient for ConsumerClientBuilder {
    type Client = ConsumerClient;

    async fn with_connection_string(
        self,
        connection_string: &str,
        event_hub_name: &str,
    ) -> azure_messaging_eventhubs::Result<ConsumerClient> {
        self.open_with_connection_string(connection_string, Some(event_hub_name))
            .await
    }

    async fn with_credential(
        self,
        fully_qualified_namespace: &str,
        event_hub_name: &str,
        credential: Arc<dyn TokenCredential>,
    ) -> azure_messaging_eventhubs::Result<ConsumerClient> {
        self.open(
            fully_qualified_namespace,
            event_hub_name.to_owned(),
            credential,
        )
        .await
    }
}

impl ResolvedTarget {
    /// Opens `builder`'s client with this target's auth, building the Entra
    /// ID credential (with `client_secret` for a client-secret credential)
    /// when the auth is not a connection string.
    ///
    /// # Errors
    /// Fails when the credential cannot be built or the SDK refuses to open
    /// the client; the error names the namespace and event hub only.
    pub(crate) async fn open<B: OpenClient>(
        &self,
        builder: B,
        client_secret: Option<SecretString>,
    ) -> anyhow::Result<B::Client> {
        let namespace = &self.fully_qualified_namespace;
        let hub = &self.event_hub_name;
        let opened = match &self.auth {
            ClientAuth::ConnectionString(connection_string) => {
                builder
                    .with_connection_string(connection_string.expose(), hub)
                    .await
            }
            ClientAuth::Credential(kind) => {
                let credential = build_credential(kind, client_secret).with_context(|| {
                    format!("Failed to build a credential for '{namespace}/{hub}'")
                })?;
                builder.with_credential(namespace, hub, credential).await
            }
        };
        opened
            .with_context(|| format!("Failed to open an Event Hubs client for '{namespace}/{hub}'"))
    }
}

/// The auth method's name for logs, never a secret.
pub(crate) fn auth_method_name(auth: &AuthMethod) -> &'static str {
    match auth {
        AuthMethod::ConnectionString { .. } => "connection-string",
        AuthMethod::Credential { .. } => "credential",
        AuthMethod::KeyVaultConnectionString { .. } => "key-vault-connection-string",
    }
}

/// Validates `target`, opens a consumer client for it, and reads the event
/// hub's partition list.
///
/// Opening only establishes the AMQP connection; authorization happens on the
/// first service call, so the partition listing is what proves the credential
/// works. For Key Vault auth the connection string is fetched first and never
/// stored beyond this call.
///
/// # Errors
/// Fails when the target is incomplete or inconsistent, when the namespace
/// host is outside [`EVENT_HUBS_ALLOWED_HOST_SUFFIXES`], when a credential
/// cannot be built or the Key Vault fetch fails, and when the service rejects
/// the connection or the credential. Errors name the namespace and event hub
/// and never contain connection-string or secret material.
pub async fn connect(target: &EventHubTarget) -> anyhow::Result<EventHubConnection> {
    let resolved = resolve_target(target).await?;
    info!(
        namespace = %resolved.fully_qualified_namespace,
        event_hub = %resolved.event_hub_name,
        "Connecting to Event Hubs"
    );

    let builder = ConsumerClient::builder().with_consumer_group(target.consumer_group.clone());
    let client = resolved.open(builder, target.client_secret.clone()).await?;
    let ResolvedTarget {
        fully_qualified_namespace: namespace,
        event_hub_name: hub,
        ..
    } = resolved;

    let properties = match client.get_eventhub_properties().await {
        Ok(properties) => properties,
        Err(error) => {
            close_client_quietly(client).await;
            return Err(anyhow::Error::new(error).context(format!(
                "Failed to read the event hub properties of '{namespace}/{hub}'"
            )));
        }
    };
    let created_on = properties.created_on.and_then(system_time_to_utc);
    info!(
        namespace = %namespace,
        event_hub = %hub,
        partitions = properties.partition_ids.len(),
        created_on = %created_on.map_or_else(|| "not reported".to_owned(), |time| time.to_rfc3339()),
        "Connected to Event Hubs"
    );

    Ok(EventHubConnection {
        client,
        fully_qualified_namespace: namespace,
        event_hub_name: hub,
        created_on,
        partition_ids: properties.partition_ids,
    })
}

/// Closes `client`, logging rather than returning a failure. Dropping a
/// client does not close its connection, so every path that abandons one
/// closes it here.
pub(crate) async fn close_client_quietly(client: ConsumerClient) {
    if let Err(error) = client.close().await {
        // Best-effort cleanup: the caller's result (an error being returned,
        // or a finished run) is already decided.
        warn!(error = %error, "Failed to close the Event Hubs client");
    }
}

/// Validates `target` and settles where its auth comes from, fetching the
/// connection string from Key Vault for Key Vault auth.
///
/// # Errors
/// Fails as [`connect`] does before it opens a client; errors never contain
/// connection-string or secret material.
pub(crate) async fn resolve_target(target: &EventHubTarget) -> anyhow::Result<ResolvedTarget> {
    match &target.auth {
        AuthMethod::ConnectionString { connection_string } => {
            resolve_connection_string(target, connection_string.clone())
        }
        AuthMethod::Credential { kind } => resolve_credential(target, kind),
        AuthMethod::KeyVaultConnectionString {
            vault_url,
            secret_name,
            credential,
        } => {
            let vault_credential = build_credential(credential, target.client_secret.clone())
                .context("Failed to build the Key Vault credential")?;
            let connection_string = fetch_secret(vault_url, secret_name, vault_credential)
                .await
                .with_context(|| {
                    format!("Failed to fetch the connection string secret '{secret_name}'")
                })?;
            resolve_connection_string(target, connection_string).with_context(|| {
                format!("The connection string in Key Vault secret '{secret_name}' is not usable")
            })
        }
    }
}

fn resolve_credential(
    target: &EventHubTarget,
    kind: &CredentialKind,
) -> anyhow::Result<ResolvedTarget> {
    let namespace = target
        .fully_qualified_namespace
        .as_deref()
        .context("A fully qualified namespace is required for credential auth")?;
    let namespace = validated_namespace(namespace)?;
    let hub = target
        .event_hub_name
        .as_deref()
        .context("An event hub name is required for credential auth")?;
    let hub = validated_event_hub_name(hub)?;

    Ok(ResolvedTarget {
        fully_qualified_namespace: namespace,
        event_hub_name: hub,
        auth: ClientAuth::Credential(kind.clone()),
    })
}

fn resolve_connection_string(
    target: &EventHubTarget,
    connection_string: SecretString,
) -> anyhow::Result<ResolvedTarget> {
    let parsed = ConnectionString::from_str(connection_string.expose())
        .context("Invalid connection string")?;
    let namespace = validated_namespace(&parsed.fully_qualified_namespace)
        .context("Invalid connection string endpoint")?;

    if let Some(configured) = target.fully_qualified_namespace.as_deref() {
        let configured = validated_namespace(configured)?;
        if configured != namespace {
            anyhow::bail!(
                "The configured namespace '{configured}' does not match the connection string \
                 endpoint '{namespace}'"
            );
        }
    }

    let from_entity_path = parsed
        .entity_path
        .as_deref()
        .map(validated_event_hub_name)
        .transpose()
        .context("Invalid EntityPath in the connection string")?;
    let configured = target
        .event_hub_name
        .as_deref()
        .map(validated_event_hub_name)
        .transpose()?;
    let hub = match (configured, from_entity_path) {
        (Some(configured), Some(entity_path)) if configured != entity_path => anyhow::bail!(
            "The configured event hub '{configured}' does not match the connection string \
             EntityPath '{entity_path}'"
        ),
        (Some(hub), _) | (None, Some(hub)) => hub,
        (None, None) => anyhow::bail!(
            "An event hub name is required: the connection string has no EntityPath and none \
             was configured (namespace '{namespace}')"
        ),
    };

    Ok(ResolvedTarget {
        fully_qualified_namespace: namespace,
        event_hub_name: hub,
        auth: ClientAuth::ConnectionString(connection_string),
    })
}

/// Lowercases `namespace` and checks it against the host allowlist. A value
/// that is not shaped like a host name is rejected without being echoed,
/// because a pasted connection string or key would otherwise land in the
/// error text.
///
/// # Errors
/// Fails when `namespace` is not a host name, or not under one of
/// [`EVENT_HUBS_ALLOWED_HOST_SUFFIXES`].
pub fn validated_namespace(namespace: &str) -> anyhow::Result<String> {
    let looks_like_host = !namespace.is_empty()
        && namespace
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    if !looks_like_host {
        anyhow::bail!(
            "Invalid Event Hubs namespace: expected a host name such as \
             '<name>.servicebus.windows.net'"
        );
    }

    let namespace = namespace.to_ascii_lowercase();
    if !host_has_allowed_suffix(&namespace, &EVENT_HUBS_ALLOWED_HOST_SUFFIXES) {
        anyhow::bail!(
            "Event Hubs namespace '{namespace}' does not have one of the required suffixes: \
             {EVENT_HUBS_ALLOWED_HOST_SUFFIXES:?}"
        );
    }

    Ok(namespace)
}

/// Checks `name` against the Event Hubs naming rules (1 to 256 letters,
/// digits, `.`, `-`, `_`, starting and ending with a letter or digit). A
/// failing value is never echoed, for the same reason as
/// [`validated_namespace`].
///
/// # Errors
/// Fails when `name` breaks the naming rules.
pub fn validated_event_hub_name(name: &str) -> anyhow::Result<String> {
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_');
    let valid = (1..=MAX_EVENT_HUB_NAME_LENGTH).contains(&name.len())
        && name.chars().all(allowed)
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name.ends_with(|c: char| c.is_ascii_alphanumeric());

    if !valid {
        anyhow::bail!(
            "Invalid event hub name: use 1 to 256 letters, digits, '.', '-', or '_', starting \
             and ending with a letter or digit"
        );
    }

    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    const SECRET_KEY: &str = "c2VjcmV0LWtleS1kby1ub3QtbGVhaw==";

    fn connection_string(endpoint_host: &str, entity_path: Option<&str>) -> SecretString {
        let mut value = format!(
            "Endpoint=sb://{endpoint_host}/;SharedAccessKeyName=listen;SharedAccessKey={SECRET_KEY}"
        );
        if let Some(entity_path) = entity_path {
            value.push_str(&format!(";EntityPath={entity_path}"));
        }
        value.into()
    }

    fn connection_string_target(
        connection_string: SecretString,
        namespace: Option<&str>,
        hub: Option<&str>,
    ) -> EventHubTarget {
        EventHubTarget {
            auth: AuthMethod::ConnectionString { connection_string },
            client_secret: None,
            fully_qualified_namespace: namespace.map(str::to_owned),
            event_hub_name: hub.map(str::to_owned),
            consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        }
    }

    fn credential_target(namespace: Option<&str>, hub: Option<&str>) -> EventHubTarget {
        EventHubTarget {
            auth: AuthMethod::Credential {
                kind: CredentialKind::DeveloperTools,
            },
            client_secret: None,
            fully_qualified_namespace: namespace.map(str::to_owned),
            event_hub_name: hub.map(str::to_owned),
            consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        }
    }

    fn assert_no_secret_material(error: &anyhow::Error) {
        for rendered in [
            format!("{error}"),
            format!("{error:#}"),
            format!("{error:?}"),
        ] {
            assert!(!rendered.contains(SECRET_KEY), "leaked key: {rendered}");
            assert!(
                !rendered.contains("SharedAccessKey="),
                "leaked connection string: {rendered}"
            );
        }
    }

    async fn resolve_error(target: &EventHubTarget) -> anyhow::Error {
        match resolve_target(target).await {
            Ok(_) => panic!("expected the target to be rejected"),
            Err(error) => error,
        }
    }

    #[tokio::test]
    async fn connection_string_target_takes_namespace_and_hub_from_the_string() {
        let target = connection_string_target(
            connection_string("Orders.servicebus.windows.net", Some("orders")),
            None,
            None,
        );

        let resolved = resolve_target(&target).await.unwrap();

        assert_eq!(
            resolved.fully_qualified_namespace,
            "orders.servicebus.windows.net"
        );
        assert_eq!(resolved.event_hub_name, "orders");
        assert!(matches!(resolved.auth, ClientAuth::ConnectionString(_)));
    }

    #[tokio::test]
    async fn connection_string_target_accepts_a_matching_configured_hub() {
        let target = connection_string_target(
            connection_string("orders.servicebus.windows.net", Some("orders")),
            Some("orders.servicebus.windows.net"),
            Some("orders"),
        );

        let resolved = resolve_target(&target).await.unwrap();

        assert_eq!(resolved.event_hub_name, "orders");
    }

    #[tokio::test]
    async fn connection_string_target_uses_the_configured_hub_without_entity_path() {
        let target = connection_string_target(
            connection_string("orders.servicebus.windows.net", None),
            None,
            Some("orders"),
        );

        let resolved = resolve_target(&target).await.unwrap();

        assert_eq!(resolved.event_hub_name, "orders");
    }

    #[tokio::test]
    async fn credential_target_resolves_namespace_and_hub() {
        let target = credential_target(Some("orders.servicebus.usgovcloudapi.net"), Some("hub"));

        let resolved = resolve_target(&target).await.unwrap();

        assert_eq!(
            resolved.fully_qualified_namespace,
            "orders.servicebus.usgovcloudapi.net"
        );
        assert!(matches!(
            resolved.auth,
            ClientAuth::Credential(CredentialKind::DeveloperTools)
        ));
    }

    #[rstest]
    #[case::missing_namespace(None, Some("hub"), "fully qualified namespace is required")]
    #[case::missing_hub(
        Some("orders.servicebus.windows.net"),
        None,
        "event hub name is required"
    )]
    #[case::foreign_host(
        Some("evil.example.com"),
        Some("hub"),
        "'evil.example.com' does not have one of the required suffixes"
    )]
    #[case::no_dot_boundary(
        Some("evilservicebus.windows.net"),
        Some("hub"),
        "does not have one of the required suffixes"
    )]
    #[case::bad_hub_name(
        Some("orders.servicebus.windows.net"),
        Some("-starts-with-dash"),
        "Invalid event hub name"
    )]
    #[tokio::test]
    async fn credential_target_rejects_incomplete_or_untrusted_input(
        #[case] namespace: Option<&str>,
        #[case] hub: Option<&str>,
        #[case] expected: &str,
    ) {
        let error = resolve_error(&credential_target(namespace, hub)).await;

        assert!(format!("{error:#}").contains(expected), "{error:#}");
    }

    #[tokio::test]
    async fn conflicting_hub_names_are_rejected_with_both_names() {
        let target = connection_string_target(
            connection_string("orders.servicebus.windows.net", Some("orders")),
            None,
            Some("payments"),
        );

        let error = resolve_error(&target).await;

        let rendered = format!("{error:#}");
        assert!(rendered.contains("'payments'"), "{rendered}");
        assert!(rendered.contains("'orders'"), "{rendered}");
    }

    #[tokio::test]
    async fn conflicting_namespaces_are_rejected() {
        let target = connection_string_target(
            connection_string("orders.servicebus.windows.net", Some("orders")),
            Some("other.servicebus.windows.net"),
            None,
        );

        let error = resolve_error(&target).await;

        assert!(format!("{error:#}").contains("does not match"), "{error:#}");
    }

    #[tokio::test]
    async fn missing_hub_name_is_rejected_for_connection_strings() {
        let target = connection_string_target(
            connection_string("orders.servicebus.windows.net", None),
            None,
            None,
        );

        let error = resolve_error(&target).await;

        assert!(
            format!("{error:#}").contains("event hub name is required"),
            "{error:#}"
        );
        assert_no_secret_material(&error);
    }

    #[tokio::test]
    async fn connection_string_endpoint_outside_the_allowlist_is_rejected_without_leaking() {
        let target = connection_string_target(
            connection_string("attacker.example.com", Some("orders")),
            None,
            None,
        );

        let error = resolve_error(&target).await;

        assert!(
            format!("{error:#}").contains("'attacker.example.com' does not have one of"),
            "{error:#}"
        );
        assert_no_secret_material(&error);
    }

    #[rstest]
    #[case::missing_key(format!("Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKeyName={SECRET_KEY}"))]
    #[case::empty_key(format!("Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKeyName=listen;SharedAccessKey=;EntityPath={SECRET_KEY}"))]
    #[case::no_endpoint(format!("SharedAccessKeyName=listen;SharedAccessKey={SECRET_KEY}"))]
    #[case::garbage(format!("not a connection string {SECRET_KEY}"))]
    #[tokio::test]
    async fn malformed_connection_strings_are_rejected_without_echoing_input(#[case] raw: String) {
        let target = connection_string_target(raw.into(), None, Some("orders"));

        let error = resolve_error(&target).await;

        assert!(
            format!("{error:#}").contains("Invalid connection string"),
            "{error:#}"
        );
        assert_no_secret_material(&error);
    }

    #[tokio::test]
    async fn a_secret_pasted_into_the_hub_field_is_not_echoed() {
        let target = connection_string_target(
            connection_string("orders.servicebus.windows.net", None),
            None,
            Some(&format!("SharedAccessKey={SECRET_KEY}")),
        );

        let error = resolve_error(&target).await;

        assert!(
            format!("{error:#}").contains("Invalid event hub name"),
            "{error:#}"
        );
        assert_no_secret_material(&error);
    }

    #[tokio::test]
    async fn a_secret_pasted_into_the_namespace_field_is_not_echoed() {
        let target = credential_target(
            Some(&format!("Endpoint=sb://x/;SharedAccessKey={SECRET_KEY}")),
            Some("orders"),
        );

        let error = resolve_error(&target).await;

        assert!(
            format!("{error:#}").contains("Invalid Event Hubs namespace"),
            "{error:#}"
        );
        assert_no_secret_material(&error);
    }

    #[tokio::test]
    async fn key_vault_target_with_client_secret_kind_and_no_secret_fails_before_any_fetch() {
        let target = EventHubTarget {
            auth: AuthMethod::KeyVaultConnectionString {
                vault_url: "https://my-vault.vault.azure.net".to_owned(),
                secret_name: "eh-connection".to_owned(),
                credential: CredentialKind::ClientSecret {
                    tenant_id: "tenant".to_owned(),
                    client_id: "client".to_owned(),
                },
            },
            client_secret: None,
            fully_qualified_namespace: None,
            event_hub_name: None,
            consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        };

        let error = resolve_error(&target).await;

        assert!(
            format!("{error:#}").contains("Client secret must be provided"),
            "{error:#}"
        );
    }

    #[tokio::test]
    async fn key_vault_target_with_a_foreign_vault_host_fails_before_any_fetch() {
        let target = EventHubTarget {
            auth: AuthMethod::KeyVaultConnectionString {
                vault_url: "https://evil.example.com".to_owned(),
                secret_name: "eh-connection".to_owned(),
                credential: CredentialKind::DeveloperTools,
            },
            client_secret: None,
            fully_qualified_namespace: None,
            event_hub_name: None,
            consumer_group: DEFAULT_CONSUMER_GROUP.to_owned(),
        };

        let error = resolve_error(&target).await;

        let rendered = format!("{error:#}");
        assert!(rendered.contains("'eh-connection'"), "{rendered}");
        assert!(rendered.contains("evil.example.com"), "{rendered}");
    }

    #[rstest]
    #[case::single_char("a", true)]
    #[case::dots_dashes_underscores("orders.v2-in_bound", true)]
    #[case::max_length(&"a".repeat(256), true)]
    #[case::too_long(&"a".repeat(257), false)]
    #[case::empty("", false)]
    #[case::trailing_dot("orders.", false)]
    #[case::leading_underscore("_orders", false)]
    #[case::space("my orders", false)]
    #[case::equals_sign("key=value", false)]
    fn validated_event_hub_name_follows_the_naming_rules(
        #[case] name: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(validated_event_hub_name(name).is_ok(), expected);
    }
}
