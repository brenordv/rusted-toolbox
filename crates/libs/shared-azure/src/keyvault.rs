use anyhow::Context;
use azure_core::credentials::TokenCredential;
use azure_security_keyvault_secrets::SecretClient;
use common_utils::string_utils::SecretString;
use std::sync::Arc;

const AZURE_KEY_VAULT_ALLOWED_URL_SUFFIXES: [&str; 4] = [
    "vault.azure.net",
    "vault.azure.cn",
    "vault.usgovcloudapi.net",
    "vault.microsoftazure.de",
];

pub async fn fetch_secret(
    vault_url: &str,
    secret_name: &str,
    credential: Arc<dyn TokenCredential>,
) -> anyhow::Result<SecretString> {
    if vault_url.trim().is_empty() {
        return Err(anyhow::anyhow!("Vault URL must be provided"));
    }

    let vault_uri = url::Url::parse(vault_url).context("Failed to parse Azure Key Vault URL")?;

    let normalized_vault_url = vault_uri
        .host_str()
        .context("Failed to extract the host from the Azure Key Vault URL")?
        .to_ascii_lowercase();

    if vault_uri.scheme() != "https" {
        return Err(anyhow::anyhow!("Vault URL must use the 'https' scheme"));
    }

    if !AZURE_KEY_VAULT_ALLOWED_URL_SUFFIXES
        .iter()
        .any(|suffix| normalized_vault_url.ends_with(&format!(".{}", suffix)))
    {
        return Err(anyhow::anyhow!(
            "Vault URL must have one of the following suffixes: {:?}",
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
        .context("Failed to fetch secret from Azure Key Vault")?
        .into_model()
        .context("Failed to convert secret response into model (azure_core::credentials::Secret)")?
        .value
        .context(format!(
            "Secret value is empty. Possibly '{}' exists, but has no value",
            secret_name
        ))?
        .into();

    Ok(secret)
}
