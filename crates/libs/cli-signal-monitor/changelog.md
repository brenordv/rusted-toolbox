# Changelog

## 1.0.1
- Wrote the readme, which was an empty file: polling contract, `immediate_exit`
  semantics (exit 0, skipped `Drop` and its telemetry-loss consequence), and the
  once-per-process constraint.

## 1.0.0
- Shutdown messages now go to stderr instead of stdout, keeping stdout a clean
  data channel for piped consumers.
- Handler installation failures carry context and print the full error chain.
- Removed the `cargo new` scaffolding (`add` and its test).
- Added crate and function documentation and the first real test.
- Initial release: Ctrl-C graceful-shutdown helper.