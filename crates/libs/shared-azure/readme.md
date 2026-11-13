# shared-azure

Azure plumbing shared by the tools: build an Entra ID token credential from a
described kind, and fetch secrets from Key Vault. This crate owns the
workspace's Azure SDK dependency set (`azure_core`, `azure_identity`,
`azure_security_keyvault_secrets`; default features, so rustls and no
OpenSSL). Everything network-facing is async and the crate does not own a
runtime; callers bring tokio.

## The three auth paths

`AuthMethod` describes how a tool authenticates to an Azure service:

- `ConnectionString`: a full connection string, held in a redacting
  `SecretString` (from `common-utils`).
- `Credential`: an Entra ID credential described by `CredentialKind`:
  `DeveloperTools` (chains Azure CLI and Azure Developer CLI; the
  local-development path), `ManagedIdentity` (for code running on Azure), or
  `ClientSecret` (service principal).
- `KeyVaultConnectionString`: the connection string lives in a Key Vault
  secret; authenticate to the vault with a `CredentialKind`, then fetch the
  named secret.

`AuthMethod` and `CredentialKind` never hold raw secret material (the
`ClientSecret` secret travels as a separate parameter, and connection strings
sit inside the redacting newtype), so both are safe to `Debug` and clone.
Neither implements serde: a struct that embeds them and derives `Serialize`
fails to compile, which keeps secrets out of accidental JSON output.

## Building a credential

```rust,ignore
use shared_azure::auth::{build_credential, CredentialKind};

let credential = build_credential(&CredentialKind::DeveloperTools, None)?;
```

For `ClientSecret`, the secret is passed separately as a `SecretString`:

```rust,ignore
let kind = CredentialKind::ClientSecret {
    tenant_id: "...".into(),
    client_id: "...".into(),
};
let credential = build_credential(&kind, Some(client_secret))?;
```

Constructing a credential does not contact Azure; tokens are acquired on
first use.

## Fetching a secret

```rust,ignore
use shared_azure::key_vault::fetch_secret;

let secret = fetch_secret(
    "https://my-vault.vault.azure.net",
    "eventhub-connection-string",
    credential,
)
.await?;
let raw = secret.expose(); // the only way to reach the value
```

`fetch_secret` returns the latest version of the secret as a `SecretString`.
Error messages name the secret, never its value.

## Vault URL validation

`fetch_secret` rejects, before any network call, a vault URL that is not
`https` or whose host does not sit under a known Key Vault DNS suffix
(`vault.azure.net`, `vault.azure.cn`, `vault.usgovcloudapi.net`,
`vault.microsoftazure.de`). Tools auto-load config from the working
directory, so a planted config could otherwise point the tool, and the AAD
token it acquires, at an arbitrary host. Additional or private suffixes are
deliberately not configurable.

## Manual verification (not CI)

The default test suite never contacts Azure. One `#[ignore]`d test proves the
fetch error names the vault host and secret name by dialing a nonexistent
vault host; it rides the SDK's retry backoff for over a minute, so it only
runs on demand: `cargo test -p shared-azure -- --ignored`.

To exercise the real path against a dev vault:

1. `az login`
2. Build a `DeveloperTools` credential and call `fetch_secret` with the dev
   vault URL and a known secret name.
3. Confirm the value arrives, and that `{:?}` and `{}` of the result both
   print `<redacted>`.