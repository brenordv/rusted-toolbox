# Changelog

## 1.0.0
- Initial release.
- Azure SDK pins: `azure_core` 1.2.0-beta.2, `azure_identity` 1.1.0-beta.2,
  `azure_security_keyvault_secrets` 1.1.0-beta.2. These share one `azure_core` with
  `azure_messaging_eventhubs` 0.16.0, so a credential built here opens Event Hubs clients
  directly.
