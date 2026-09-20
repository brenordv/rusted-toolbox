use std::sync::Arc;
use azure_core::credentials::TokenCredential;
use azure_security_keyvault_secrets::SecretClient;
use common_utils::string_utils::SecretString;

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

    let l_vault_url = vault_url.trim().to_ascii_lowercase();

    if !AZURE_KEY_VAULT_ALLOWED_URL_SUFFIXES.iter().any(|suffix| l_vault_url.ends_with(suffix)) {
        return Err(anyhow::anyhow!("Vault URL must have one of the following suffixes: {:?}", AZURE_KEY_VAULT_ALLOWED_URL_SUFFIXES));
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