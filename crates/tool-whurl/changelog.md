# 1.2.0 (2025-11-14)
- Added runtime dynamic variables (`.dvars`) with generators for timestamps, random data, and optional shell execution.
- Introduced the `# @vars` directive and automatic loading of `_global.dvars` and `<env>.dvars` across include graphs.
- Hardened `$shell(...)` dynamic values with an allow-list toggle, cross-platform shell selection, and destructive command detection.
- Require at least one `<env>.hurlvars` or `<env>.dvars` file whenever `--env` is used, preventing partially defined environments.
- Renamed `--print-only-result` to `--print-only-full-response` and now pretty-print the JSON report when streamed to stdout.
- Added a `--print-only-response-body` mode to emit just the final response body without headers or logs.
- Ensure a blank line is printed for 204 responses so the terminal output reflects execution.

# 1.1.0 (2025-11-11)
- Added support for per-API `_global.hurlvars` files that always load alongside the selected environment.
- Automatically load env/global variable files for cross-API includes to keep shared requests in sync.
- Emit warnings (when not in silent mode) whenever variable sources collide during merge.
- Documented global variables and clarified `--file-root` usage with practical examples in the README.

# 1.0.0 (2025-11-09)
- Initial release.