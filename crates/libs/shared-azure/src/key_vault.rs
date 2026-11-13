//! Key Vault secret retrieval with vault-host validation.

use anyhow::Context;
use azure_core::credentials::TokenCredential;
use azure_security_keyvault_secrets::SecretClient;
use common_utils::string_utils::SecretString;
use std::sync::Arc;

/// DNS suffixes a vault host must sit under, per Azure cloud: public, China
/// (21Vianet), and US Government. The live
/// list is the "DNS suffixes for object identifiers" table in
/// <https://learn.microsoft.com/en-us/azure/key-vault/general/about-keys-secrets-certificates>.
const AZURE_KEY_VAULT_ALLOWED_URL_SUFFIXES: [&str; 3] = [
    "vault.azure.net",
    "vault.azure.cn",
    "vault.usgovcloudapi.net",
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
        .with_context(|| format!("Secret '{}' exists, but has no value", secret_name))?
        .into();

    Ok(secret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use azure_core::credentials::{AccessToken, TokenRequestOptions};
    use azure_core::error::ErrorKind;
    use rstest::rstest;

    /// Refuses every token request, so tests can drive `fetch_secret` without
    /// a real credential flow or any network traffic.
    #[derive(Debug)]
    struct FakeCredential;

    #[async_trait::async_trait]
    impl TokenCredential for FakeCredential {
        async fn get_token(
            &self,
            _scopes: &[&str],
            _options: Option<TokenRequestOptions<'_>>,
        ) -> azure_core::Result<AccessToken> {
            Err(azure_core::Error::with_message(
                ErrorKind::Credential,
                "fake credential refuses to issue tokens",
            ))
        }
    }

    fn fake_credential() -> Arc<dyn TokenCredential> {
        Arc::new(FakeCredential)
    }

    #[rstest]
    #[case::empty_url("", "Vault URL must be provided")]
    #[case::whitespace_url("   ", "Vault URL must be provided")]
    #[case::not_a_url("not a url", "Failed to parse Azure Key Vault URL")]
    #[case::no_host("file:///tmp/secret-store", "Failed to extract the host")]
    #[case::http_scheme("http://my-vault.vault.azure.net", "must use the 'https' scheme")]
    #[case::unknown_host(
        "https://evil.example.com",
        "'evil.example.com' does not have one of the required suffixes"
    )]
    #[case::no_dot_boundary(
        "https://evilvault.azure.net",
        "'evilvault.azure.net' does not have one of the required suffixes"
    )]
    #[case::suffix_only_in_query(
        "https://evil.example.com/?x=.vault.azure.net",
        "'evil.example.com' does not have one of the required suffixes"
    )]
    #[case::bare_suffix_host(
        "https://vault.azure.net",
        "'vault.azure.net' does not have one of the required suffixes"
    )]
    #[case::userinfo_spoof(
        "https://my-vault.vault.azure.net@evil.example.com/",
        "'evil.example.com' does not have one of the required suffixes"
    )]
    #[tokio::test]
    async fn fetch_secret_rejects_invalid_vault_urls(
        #[case] vault_url: &str,
        #[case] expected_fragment: &str,
    ) {
        let error = fetch_secret(vault_url, "some-secret", fake_credential())
            .await
            .expect_err("invalid vault URL should be rejected");

        let rendered = format!("{error:#}");
        assert!(
            rendered.contains(expected_fragment),
            "unexpected error for '{vault_url}': {rendered}"
        );
    }

    #[rstest]
    #[case::empty("")]
    #[case::whitespace("  ")]
    #[tokio::test]
    async fn fetch_secret_rejects_a_blank_secret_name(#[case] secret_name: &str) {
        let error = fetch_secret(
            "https://my-vault.vault.azure.net",
            secret_name,
            fake_credential(),
        )
        .await
        .expect_err("blank secret name should be rejected");

        assert!(
            error.to_string().contains("Secret name must be provided"),
            "unexpected error: {error:#}"
        );
    }

    #[ignore = "dials a nonexistent vault host and rides the SDK retry backoff (~80s); run with --ignored"]
    #[tokio::test]
    async fn fetch_secret_error_names_host_and_secret_but_never_secret_material() {
        let vault_url = "https://rusted-toolbox-vault-that-does-not-exist.vault.azure.net";

        let error = fetch_secret(vault_url, "my-secret-name", fake_credential())
            .await
            .expect_err("a vault the credential refuses should fail");

        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("rusted-toolbox-vault-that-does-not-exist.vault.azure.net"),
            "host missing from error: {rendered}"
        );
        assert!(
            rendered.contains("my-secret-name"),
            "secret name missing from error: {rendered}"
        );
    }
}
