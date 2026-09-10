# Changelog

## 2.1.0
- Wired OpenTelemetry export through `logging-otel` (`app_boot_up_with_otel`,
  built with the `otel` feature). The endpoint comes from `--otel-endpoint`, the
  new top-level `otel_endpoint` config-file key, or the
  `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable (resolved by
  `logging-otel`). The endpoint value is never logged or printed; the header
  shows a presence-only "OpenTelemetry export: configured/not configured"
  line. `main`
  drops the returned guard before calling the exit helpers so buffered
  telemetry flushes.
- On the OTel path, console log output goes to stdout (raccoon-otel) instead of
  the stderr default.
- Readme: `--verbose` was described as enabling verbose logs, but nothing reads it; it is now
  listed among the shared flags as accepted and unused. The config-example note no longer claims
  `config.example.json` is an exact copy of the readme example (the file's connectivity intervals
  are shorter).
- Moved `otel_endpoint` in `config.example.json` to the top level; nested under
  `notifications` it was silently dropped during deserialization.
- Fixed `--min-download-threshold` being ignored: the download minimum was read
  from the upload flag.
- Fixed `--replace-urls` being parsed but never applied. The CLI path now uses
  the same URL semantics as the config file: without the flag, user URLs merge
  with the default list; with it, they replace the defaults. Either way the
  list is deduplicated and falls back to the defaults when empty. Previously
  the CLI path used only the user URLs and never merged the defaults.
- Fixed CLI mode refusing to start when the database file did not exist; the
  database (and missing parent directories) are created on startup, matching
  the config-file path.
- `--expected-upload` is now optional in CLI mode, as documented; it was
  required unless a config file was used, which made download-only mode
  unreachable from the CLI.
- Threshold category values on the CLI now use the documented snake_case names
  (`very_slow`, `medium_fast`, ...) and parse case-insensitively; they were
  kebab-case (`very-slow`) despite the help text.
- The `--config` flag now actually conflicts with every tool-specific flag; most
  entries in the conflict list used the flag spelling instead of the argument
  ID, so the conflicts were never enforced.
- The header's cleanup interval line now prints the underlying duration in
  seconds (for example `3600s`) instead of whole days, which showed "0 day(s)"
  for the CLI default of 3600 seconds.
- Fixed the upload-thresholds parse error saying "download thresholds".
- Header tool section now renders through `common-cli`'s `header_format`
  helpers; output bytes are unchanged apart from the cleanup interval and the
  new OpenTelemetry line.
- Added tests: threshold parsing, config-file resolvers, connectivity outage
  state machine, and the CLI regression cases above.

## 2.0.0
- Refactored to fit the new tooling.

## 1.1.0
- Version as imported into this workspace; changes before the import were not
  tracked here.

## 1.0.0
Initial release