# 2.0.0
- Refactored to fit the new tooling: CLI parsing now uses the `clap` derive API and the shared `CommonToolArgs`, replacing the hand-built `Command` from the retired `shared` crate.
- Gained the shared runtime flags: `-L/--log-level` (case insensitive), `--app-header`, `--log-to-console`, `--log-to-file`, and `--rotate-log-file-by-day`.
- Argument errors (invalid `--date`, `-t`, `--reference`, `--time`, or more than one time source) now surface through the shared logging pipeline; exit codes are explicit: `0` on success and `1` on failure.

# 1.0.1
- Updated dependencies.

# 1.0.0
- Initial release.