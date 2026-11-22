//! Credential kinds and construction for Entra ID authentication.

use common_utils::string_utils::SecretString;
use std::sync::Arc;

/// How to build a [`TokenCredential`](azure_core::credentials::TokenCredential).
/// Carries no secret material, so values are safe to `Debug`, clone, and embed
/// in config models.
#[derive(Debug, Clone)]
pub enum CredentialKind {
    /// `azure_identity::DeveloperToolsCredential`: chains the local developer
    /// tools (Azure CLI, Azure Developer CLI). The local-development path.
    DeveloperTools,
    /// `azure_identity::ManagedIdentityCredential`. The path for code running
    /// on Azure compute.
    ManagedIdentity,
    /// `azure_identity::ClientSecretCredential` (service principal). The
    /// secret itself is not part of the enum; it travels as a separate
    /// parameter to [`build_credential`].
    ClientSecret {
        tenant_id: String,
        client_id: String,
    },
}

/// The ways a tool can authenticate to an Azure service.
#[derive(Debug, Clone)]
pub enum AuthMethod {
    /// A full connection string, held redacted.
    ConnectionString { connection_string: SecretString },
    /// An Entra ID credential described by [`CredentialKind`].
    Credential { kind: CredentialKind },
    /// The connection string lives in a Key Vault secret: authenticate to the
    /// vault with `credential`, then fetch `secret_name` from `vault_url`.
    KeyVaultConnectionString {
        vault_url: String,
        secret_name: String,
        /// How we authenticate to the vault itself.
        credential: CredentialKind,
    },
}

/// Builds a ready-to-use [`TokenCredential`](azure_core::credentials::TokenCredential)
/// for `kind`.
///
/// `client_secret` is required by [`CredentialKind::ClientSecret`] and ignored
/// by the other kinds; it travels as a separate [`SecretString`] so the enum
/// itself never holds secret material.
///
/// Construction does not contact Azure; tokens are acquired on first use.
///
/// # Errors
/// Fails when `kind` is `ClientSecret` and the secret, tenant ID, or client ID
/// is missing or blank, or when the underlying `azure_identity` constructor
/// rejects its input.
pub fn build_credential(
    kind: &CredentialKind,
    client_secret: Option<SecretString>,
) -> anyhow::Result<Arc<dyn azure_core::credentials::TokenCredential>> {
    match kind {
        CredentialKind::DeveloperTools => {
            let credentials = azure_identity::DeveloperToolsCredential::new(None)?;

            Ok(credentials)
        }
        CredentialKind::ManagedIdentity => {
            let credentials = azure_identity::ManagedIdentityCredential::new(None)?;

            Ok(credentials)
        }
        CredentialKind::ClientSecret {
            tenant_id,
            client_id,
        } => {
            let Some(client_secret) = client_secret else {
                return Err(anyhow::anyhow!(
                    "Client secret must be provided for ClientSecret credential"
                ));
            };

            if tenant_id.trim().is_empty() {
                return Err(anyhow::anyhow!(
                    "Tenant ID must be provided for ClientSecret credential"
                ));
            }

            if client_id.trim().is_empty() {
                return Err(anyhow::anyhow!(
                    "Client ID must be provided for ClientSecret credential"
                ));
            }

            let client_secret_value = client_secret.expose().to_owned();

            if client_secret_value.trim().is_empty() {
                return Err(anyhow::anyhow!(
                    "Client secret must be provided for ClientSecret credential"
                ));
            }

            let secret = azure_core::credentials::Secret::new(client_secret_value);

            let credentials = azure_identity::ClientSecretCredential::new(
                tenant_id,
                client_id.clone(),
                secret,
                None,
            )?;

            Ok(credentials)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn build_credential_constructs_developer_tools_without_a_secret() {
        let credential = build_credential(&CredentialKind::DeveloperTools, None);

        assert!(credential.is_ok(), "{:?}", credential.err());
    }

    #[test]
    fn build_credential_constructs_managed_identity_without_a_secret() {
        let credential = build_credential(&CredentialKind::ManagedIdentity, None);

        assert!(credential.is_ok(), "{:?}", credential.err());
    }

    #[test]
    fn build_credential_constructs_client_secret_with_all_parts() {
        let kind = CredentialKind::ClientSecret {
            tenant_id: "11111111-2222-3333-4444-555555555555".to_string(),
            client_id: "66666666-7777-8888-9999-000000000000".to_string(),
        };

        let credential = build_credential(&kind, Some("some-client-secret".to_string().into()));

        assert!(credential.is_ok(), "{:?}", credential.err());
    }

    #[rstest]
    #[case::missing_secret(None, "tenant", "client", "Client secret must be provided")]
    #[case::blank_secret(Some("   "), "tenant", "client", "Client secret must be provided")]
    #[case::blank_tenant(Some("secret"), "  ", "client", "Tenant ID must be provided")]
    #[case::blank_client(Some("secret"), "tenant", "", "Client ID must be provided")]
    fn build_credential_client_secret_rejects_missing_parts(
        #[case] secret: Option<&str>,
        #[case] tenant_id: &str,
        #[case] client_id: &str,
        #[case] expected_fragment: &str,
    ) {
        let kind = CredentialKind::ClientSecret {
            tenant_id: tenant_id.to_string(),
            client_id: client_id.to_string(),
        };

        let error = build_credential(&kind, secret.map(|s| s.to_string().into()))
            .expect_err("construction should fail");

        assert!(
            error.to_string().contains(expected_fragment),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn auth_method_debug_shows_non_secret_fields_and_redacts_the_connection_string() {
        let methods = [
            AuthMethod::ConnectionString {
                connection_string: "Endpoint=sb://x/;SharedAccessKey=sekret".to_string().into(),
            },
            AuthMethod::Credential {
                kind: CredentialKind::DeveloperTools,
            },
            AuthMethod::KeyVaultConnectionString {
                vault_url: "https://my-vault.vault.azure.net".to_string(),
                secret_name: "eh-connection-string".to_string(),
                credential: CredentialKind::ManagedIdentity,
            },
        ];

        let rendered = format!("{methods:?}");

        assert!(!rendered.contains("sekret"), "leaked: {rendered}");
        assert!(!rendered.contains("Endpoint="), "leaked: {rendered}");
        assert!(rendered.contains("<redacted>"));
        assert!(rendered.contains("my-vault.vault.azure.net"));
        assert!(rendered.contains("eh-connection-string"));
        assert!(rendered.contains("DeveloperTools"));
    }
}
