# 2.0.0
- Refactored to fit the new tooling: CLI parsing now uses the `clap` derive API and the shared `CommonToolArgs`, replacing the hand-built `Command` from the retired `shared` crate.
- `--wait` can now be given without a value to listen on the default port (2428).
- Logging is now controlled at runtime through the shared flags (`-L/--log-level`, `--log-to-file`, `--rotate-log-file-by-day`, `--log-to-console`) instead of compile-time toggles; use `--log-to-file` to capture a debug log without disturbing the chat UI.
- Gained the shared `--app-header` flag, which prints the runtime configuration before the session starts.

# 1.0.1
- Updated dependencies.

# 1.0.0
- Initial release.