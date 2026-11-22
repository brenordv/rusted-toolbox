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
  form can check a namespace or event hub name with the same rules a run applies, before
  connecting. Neither echoes a rejected value.
- A stored checkpoint ahead of its partition's last sequence number (a recreated event hub,
  empty or not) fails that partition at start with a message naming both numbers, instead of
  opening a receiver the service rejects with the same comparison. A stream error names the
  SDK's innermost error once instead of repeating it through the error chain.
- `filters::parse_time`, `parse_time_with_granularity`, `parse_window`, `TimeWindow`, and
  `TIME_SHAPES` parse capture-rule timestamps (an RFC3339 timestamp or a date; During needs a
  full timestamp; a window's start may not follow its end), so the CLI and the GUI accept the
  same input.
- `producer`: `open_producer` on the same target validation and auth dispatch as a run,
  `EventHubProducer::send` (batched or paced with `Pacing::interval`, partition-key routing,
  cancel before a batch, during its creation, or during a paced wait, counts and end reason
  in `SendSummary`), and `close`. `cancel::flag_set` and `CANCEL_POLL` wait on a cancel flag.
- `connection::target_names` returns the namespace and event hub a target resolves to, by the
  rules a run and `open_producer` apply, without any network call (`None` for Key Vault
  auth), so a form can name the destination before sending.
- `producer::next_event` builds the event to send: the body as given, or a fresh render of a
  `mock_data_utils` template under the same content type, with the caller's `now` as the time
  `{{now}}` and `{{utc-now}}` show and the caller's `sequence` as the number `{{sequence}}`
  shows. Adds the `mock-data-utils` dependency.
- `RunSummary::failed_partitions` lists the partitions that ended in `Failed`, in map order.
- `OutgoingEvent` carries a correlation id and string application properties (and derives
  `Default`); `next_event` keeps both on a rendered event. `producer::parse_property` and
  `producer::insert_property` parse `key=value` and refuse a repeated key, with errors that hold
  no value. An event too large for a batch names its property count and whether it carries a
  correlation id.
- `TextFilter::keywords` builds a keyword filter with each item trimmed and empty items
  dropped (`None` when no keyword is left), since an empty keyword matches every body. A saved
  keyword filter loads through the same rule, and one with no keyword left fails to load.
- `tls::install_crypto_provider` sets rustls's aws-lc-rs provider as the process default (and
  says whether it was the call that set it). A binary built alongside crates that enable
  rustls's `ring` provider has no default otherwise, and the Event Hubs connection's
  `ClientConfig::builder()` panics. Adds the `rustls` 0.23.46 dependency.
- An `ignore_checkpoint` run advances the stored checkpoint only when it covers it: it starts at
  the earliest event, or the first event a partition processes is at most one sequence number
  past the checkpoint. A run that starts further on stores every event with the existing
  checkpoint and logs one warning per partition, so the next normal run reads what lies between.
- The engine reads commands while it connects. `EngineCommand::Stop` then ends the run with
  `RunEnd::Stopped`, no partitions, and no `Started` event (a client already open is closed);
  `UpdateFilters` then replaces the filters the run starts with.
- Azure SDK pins: `azure_messaging_eventhubs` 0.16.0 on `azure_core`
  1.2.0-beta.2 (one `azure_core` shared with `shared-azure`'s
  `azure_identity` 1.1.0-beta.2 and `azure_security_keyvault_secrets`
  1.1.0-beta.1). `azure_core_amqp` 1.2.0-beta.3 is a dev-dependency for test
  fixtures.
