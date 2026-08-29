# 2.0.0
- Refactored to fit the new tooling.
- Rewrote CLI parsing with the `clap` derive API and the shared `CommonToolArgs`, replacing the
  hand-built `Command` from the retired `shared` crate.
- Dropped the `-n/--no-header` flag. The runtime configuration is now shown on demand with the
  shared `--app-header` flag.
- Gained the shared runtime flags: `-L/--log-level` (case insensitive), `--app-header`,
  `--log-to-console`, `--log-to-file`, and `--rotate-log-file-by-day`.
- Logging is now installed through the shared `AppLogger` during boot-up instead of a standalone
  initializer.
- Exit codes are explicit: `0` on success and `1` on failure.
- An incomplete wifi payload now reports the specific missing field (SSID or password) instead of a
  generic message.

# 1.0.0
- Initial release