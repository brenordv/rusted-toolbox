//! Key Vault secret retrieval with vault-host validation.

use anyhow::Context;
use azure_core::credentials::TokenCredential;
use azure_security_keyvault_secrets::SecretClient;
use common_utils::string_utils::SecretString;
use std::sync::Arc;

/// DNS suffixes a vault host must sit under, per Azure cloud: public, China
/// (21Vianet), US Government, plus the retired Azure Germany cloud. The live
/// list is the "DNS suffixes for object identifiers" table in
/// <https://learn.microsoft.com/en-us/azure/key-vault/general/about-keys-secrets-certificates>.
const AZURE_KEY_VAULT_ALLOWED_URL_SUFFIXES: [&str; 4] = [
    "vault.azure.net",
    "vault.azure.cn",
    "vault.usgovcloudapi.net",
    "vault.microsoftazure.de",
];

/// Fetches the latest version of `secret_name` from the Key Vault at
/// `vault_url`, returning it as a redacting [`SecretString`].
///
/// The URL is validated before any network call: it must parse, use the
/// `https` scheme, and its host must sit under one of the known Key Vault DNS
/// suffixes. Tools auto-load config from the working directory, so a planted
/// config could otherwise point the AAD token this call presents at an
/// arbitrary host. Additional or private suffixes are deliberately not
/// configurable.
///
/// # Errors
/// Fails on an empty or invalid vault URL, an empty secret name, any Key
/// Vault error, and a secret that exists but has no value. Error messages
/// name the secret, never its value.
pub async fn fetch_secret(
    vault_url: &str,
    secret_name: &str,
    credential: Arc<dyn TokenCredential>,
) -> anyhow::Result<SecretString> {
    if vault_url.trim().is_empty() {
        return Err(anyhow::anyhow!("Vault URL must be provided"));
    }

    let vault_uri = url::Url::parse(vault_url).context("Failed to parse Azure Key Vault URL")?;

    let normalized_vault_host = vault_uri
        .host_str()
        .context("Failed to extract the host from the Azure Key Vault URL")?
        .to_ascii_lowercase();

    if vault_uri.scheme() != "https" {
        return Err(anyhow::anyhow!("Vault URL must use the 'https' scheme"));
    }

    if !AZURE_KEY_VAULT_ALLOWED_URL_SUFFIXES
        .iter()
        .any(|suffix| normalized_vault_host.ends_with(&format!(".{}", suffix)))
    {
        return Err(anyhow::anyhow!(
            "Vault host '{}' does not have one of the required suffixes: {:?}",
            normalized_vault_host,
            AZURE_KEY_VAULT_ALLOWED_URL_SUFFIXES
        ));
    }

    if secret_name.trim().is_empty() {
        return Err(anyhow::anyhow!("Secret name must be provided"));
    }

    let client = SecretClient::new(vault_url, credential, None)
        .context("Failed to initialize Azure Key Vault client")?;

    let secret: SecretString = client
        .get_secret(secret_name, None)
        .await
        .with_context(|| {
            format!(
                "Failed to fetch secret '{}' from Azure Key Vault (host: '{}')",
                secret_name,
                normalized_vault_host
            )
        })?
        .into_model()
        .context("Failed to convert secret response into model (azure_security_keyvault_secrets::models::Secret)")?
        .value
        .context(format!("Secret '{}' exists, but has no value", secret_name))?
        .into();

    Ok(secret)
}
