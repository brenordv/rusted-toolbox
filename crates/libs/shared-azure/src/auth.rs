use azure_core::credentials::TokenCredential;
use azure_security_keyvault_secrets::SecretClient;
use common_utils::string_utils::SecretString;
use std::sync::Arc;

pub enum CredentialKind {
    DeveloperTools,  // azure_identity::DeveloperToolsCredential
    ManagedIdentity, // azure_identity::ManagedIdentityCredential
    ClientSecret {
        tenant_id: String,
        client_id: String,
    }, // secret passed separately
}

pub enum AuthMethod {
    ConnectionString {
        connection_string: String,
    },
    Credential {
        kind: CredentialKind,
    },
    KeyVaultConnectionString {
        vault_url: String,
        secret_name: String,
        credential: CredentialKind, // How we authenticate to the vault itself
    },
}

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
            if !client_secret.is_some() {
                return Err(anyhow::anyhow!(
                    "Client secret must be provided for ClientSecret credential"
                ));
            }

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

            let client_secret_value = client_secret.unwrap().expose().to_owned();

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

pub async fn fetch_secret(
    vault_url: &str,
    secret_name: &str,
    credential: Arc<dyn TokenCredential>,
) -> anyhow::Result<SecretString> {
    if vault_url.trim().is_empty() {
        return Err(anyhow::anyhow!("Vault URL must be provided"));
    }

    if secret_name.trim().is_empty() {
        return Err(anyhow::anyhow!("Secret name must be provided"));
    }

    let client = SecretClient::new(vault_url, credential.clone(), None)?;

    let secret: SecretString = client
        .get_secret(secret_name, None)
        .await?
        .into_model()?
        .value
        .unwrap()
        .into();

    Ok(secret)
}