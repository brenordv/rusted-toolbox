# Changelog

## 1.0.0
- Initial release: captured-message model with SDK conversion, time and text
  capture filters, validated connection for connection-string, credential, and
  Key Vault auth, the capture engine (per-partition tasks, store-then-emit,
  replay guard, early stop for closed time windows, live filter updates), and
  live statistics with per-partition progress.
- `MessageSink::bind` ties a sink to the validated event hub (`EventHubIdentity`)
  once per run, before any checkpoint is loaded; a refused bind fails the run.
  The identity and `EventHubConnection` carry the hub's service-reported
  creation time (`created_on`), so a sink can tell a recreated hub of the same
  name from the original.
- `connection::validated_namespace` and `connection::validated_event_hub_name` are public, so a
  form can check a namespace or event hub name with the same rules `connect` applies, before
  connecting. Neither echoes a rejected value.
- A stored checkpoint ahead of its partition's last sequence number (a recreated event hub,
  empty or not) fails that partition at start with a message naming both numbers, instead of
  opening a receiver the service rejects with the same comparison. A stream error names the
  SDK's innermost error once instead of repeating it through the error chain.
- `filters::parse_time`, `parse_time_with_granularity`, `parse_window`, `TimeWindow`, and
  `TIME_SHAPES` parse capture-rule timestamps (an RFC3339 timestamp or a date; During needs a
  full timestamp; a window's start may not follow its end), so the CLI and the GUI accept the
  same input.
- `producer`: `open_producer` on the same target validation and auth dispatch as `connect`,
  `EventHubProducer::send` (batched or paced with `Pacing::interval`, partition-key routing,
  cancel before a batch, during its creation, or during a paced wait, counts and end reason
  in `SendSummary`), and `close`. `cancel::flag_set` and `CANCEL_POLL` wait on a cancel flag.
- `connection::target_names` returns the namespace and event hub a target resolves to, by the
  rules `connect` and `open_producer` apply, without any network call (`None` for Key Vault
  auth), so a form can name the destination before sending.
- `producer::next_event` builds the event to send: the body as given, or a fresh render of a
  `mock_data_utils` template under the same content type. Adds the `mock-data-utils`
  dependency.
- `RunSummary::failed_partitions` lists the partitions that ended in `Failed`, in map order.
- Azure SDK pins: `azure_messaging_eventhubs` 0.16.0 on `azure_core`
  1.2.0-beta.2 (one `azure_core` shared with `shared-azure`'s
  `azure_identity` 1.1.0-beta.2 and `azure_security_keyvault_secrets`
  1.1.0-beta.1). `azure_core_amqp` 1.2.0-beta.3 is a dev-dependency for test
  fixtures.
