# Changelog

## 2.1.3
- Telegram send errors can no longer leak the bot token. The token is part of the request
  URL, and reqwest transport errors render that URL, so a connect/timeout failure put the
  token on stderr and into the OTel export via the notifier's warning. The error is now
  stripped of its URL (`reqwest::Error::without_url`) and wrapped with a token-free
  context; the warning renders the full cause chain (`{:#}`), so the transport kind
  (connect, timeout, TLS, dns) stays visible.
- The Docker runtime stage verifies the Ookla speedtest tarball against a pinned SHA256
  before extraction, and curl is restricted to HTTPS for the transfer and any redirect
  (`--proto '=https' --proto-redir '=https'`, plus `-f` so an HTTP error page fails the
  build instead of being saved). The hash is trust-on-first-use: Ookla publishes no
  signature or checksum to cross-check, so the pin was recorded from fetches on
  2026-09-12 and detects future tampering or in-place re-publishing rather than proving
  provenance. The image build was not exercised on the machine this change was written on
  (no docker CLI); the RUN chain is standard POSIX.
- Considered and deferred, recorded in next.md: digest-pinned base images (Dependabot has
  no docker ecosystem entry here, so a digest pin would freeze `ubuntu:24.04` point
  releases with nothing to refresh it) and a non-root runtime user (breaks existing
  `/data` bind mounts not writable by the chosen UID).

## 2.1.2
- A failed run is reported through `error!` like the sibling tools (it used a raw stderr
  print), and it is logged before the OTel guard drops, so the failure reaches the export
  pipeline before the flush-and-shutdown. On the active-OTel path the line lands on stdout
  (raccoon-otel's console layer), like every other log line there.
- The Docker build stage copies `rust-toolchain.toml` and builds with `--locked`, so the
  container build honors the committed lockfile and toolchain pin instead of silently
  re-resolving either when the base image drifts.
- The runtime-info header's "OpenTelemetry export" presence flag now comes from
  `logging-otel`'s own `is_otel_endpoint_configured` instead of a local re-implementation of
  the endpoint-resolution rules.

## 2.1.1
- Config and threshold validation errors are visible again: logging now boots on the failure
  path too, before the error is reported, so a bad `--config` file or threshold string reports
  its full cause chain on stderr (main prints the error with `{:#}`) instead of exiting 1 with
  no message. The error-path boot skips OpenTelemetry on purpose: the endpoint lives in the
  config that failed to resolve, and the startup-failure exit has no guard to flush.
- The Docker entrypoint no longer injects `--speedtest-cli-path` when `--config` is passed,
  so the documented config-file mode works inside the container instead of dying on the
  declared clap conflict. Config-file runs pick the bundled Ookla CLI via
  `speed.speedtest_cli_path` (the readme's Docker section shows the path); leaving it unset
  uses the embedded Cloudflare test.

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