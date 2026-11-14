# Changelog

## 1.0.0
- Initial release: captured-message model with SDK conversion, time and text
  capture filters, validated connection for connection-string, credential, and
  Key Vault auth, the capture engine (per-partition tasks, store-then-emit,
  replay guard, early stop for closed time windows, live filter updates), and
  live statistics with per-partition progress.
- `MessageSink::bind` ties a sink to the validated event hub (`EventHubIdentity`)
  once per run, before any checkpoint is loaded; a refused bind fails the run.
- `connection::validated_namespace` and `connection::validated_event_hub_name` are public, so a
  form can check a namespace or event hub name with the same rules `connect` applies, before
  connecting. Neither echoes a rejected value.
- A stored checkpoint ahead of its partition's last sequence number (a recreated event hub,
  empty or not) fails that partition at start with a message naming both numbers, instead of
  opening a receiver the service rejects with the same comparison. A stream error names the
  SDK's innermost error once instead of repeating it through the error chain.
- Azure SDK pins: `azure_messaging_eventhubs` 0.16.0 on `azure_core`
  1.2.0-beta.2 (one `azure_core` shared with `shared-azure`'s
  `azure_identity` 1.1.0-beta.2 and `azure_security_keyvault_secrets`
  1.1.0-beta.1). `azure_core_amqp` 1.2.0-beta.3 is a dev-dependency for test
  fixtures.
