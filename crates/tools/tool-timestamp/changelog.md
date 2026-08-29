# 2.0.0
- Refactored to fit the new tooling: CLI parsing now uses the `clap` derive API and the shared `CommonToolArgs`, replacing the hand-built `Command` from the retired `shared` crate.
- The runtime configuration header is now shown on demand with the shared `--app-header` flag (previously printed on every run).
- Gained the shared runtime flags: `-L/--log-level` (case insensitive), `--app-header`, `--log-to-console`, `--log-to-file`, and `--rotate-log-file-by-day`.
- Logging is now installed through the shared `AppLogger` during boot-up, and exit codes are explicit: `0` on success and `1` on failure.

# 1.1.2
- Updated dependencies.

# 1.1.1
- Removed emojis. They don't render properly on every terminal.

# 1.1.0
- Now able to process numeric timestamps with milliseconds alongside the traditional unix timestamp.

# 1.0.0
- Initial release.