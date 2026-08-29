# 2.0.0
- Refactored to fit the new tooling: CLI parsing now uses the `clap` derive API (with derived subcommands) instead of the hand-built `Command` from the retired `shared` crate.
- Logging and process-exit helpers now come from the shared `common-cli` crate; the tool keeps its own log-level resolution (Info by default, Error under `--silent`/`--print-only-*`) and its typed exit codes.
- No change to the command surface: `list`, `run`, and `dry-run` and all of their flags behave as before.

# 1.3.0
- Allow `# @vars` directives to load matching `.hurlvars` files before `.dvars`, and fail fast when neither exists (plus regression tests).
- Some house cleaning.
- Updated the macOS convenience script to only install pkg-config if it is missing.

# 1.2.1
- Fixed `--env` resolution to accept either `<env>.hurlvars` or `<env>.dvars`, instead of requiring both files to exist.

# 1.2.0
- Added runtime dynamic variables (`.dvars`) with generators for timestamps, random data, and optional shell execution.
- Introduced the `# @vars` directive and automatic loading of `_global.dvars` and `<env>.dvars` across include graphs.
- Hardened `$shell(...)` dynamic values with an allow-list toggle, cross-platform shell selection, and destructive command detection.
- Require at least one `<env>.hurlvars` or `<env>.dvars` file whenever `--env` is used, preventing partially defined environments.
- Renamed `--print-only-result` to `--print-only-full-response` and now pretty-print the JSON report when streamed to stdout.
- Added a `--print-only-response-body` mode to emit just the final response body without headers or logs.
- Ensure a blank line is printed for 204 responses so the terminal output reflects execution.

# 1.1.0
- Added support for per-API `_global.hurlvars` files that always load alongside the selected environment.
- Automatically load env/global variable files for cross-API includes to keep shared requests in sync.
- Emit warnings (when not in silent mode) whenever variable sources collide during merge.
- Documented global variables and clarified `--file-root` usage with practical examples in the README.

# 1.0.0
- Initial release.