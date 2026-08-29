# 2.0.0
- Refactored to fit the new tooling.
- Rewrote CLI parsing with the `clap` derive API and the shared `CommonToolArgs`, replacing the
  hand-built `Command` from the retired `shared` crate.
- Verbose output now uses the shared `--verbose` flag. The old `-v` short alias is gone.
- Gained the shared runtime flags: `-L/--log-level` (case-insensitive), `--app-header`,
  `--log-to-console`, `--log-to-file`, and `--rotate-log-file-by-day`.
- Logging is now installed through the shared `AppLogger` during boot-up instead of a standalone
  initializer.
- Exit codes are explicit: `0` on success and `1` on failure.

# 1.0.0 🎃
Initial release