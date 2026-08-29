# Changelog

## 1.2.0
- Exit helpers `exit_success`, `exit_error`, and `exit_with_code` now return `!`,
  so the compiler treats a call as diverging.
- Log-directory creation failure now prints a warning and skips the file layer,
  instead of being swallowed and resurfacing later as a file-open error.
- Added `CommonToolArgs::build_logger`, a public seam that builds a tool's
  `AppLogger` without installing it (consumed by the OpenTelemetry boot path).
- Added documentation across the public API and unit tests for logger filename
  resolution, `tracing_level`, `seed_rust_log`, `file_layer`, and tool-path
  resolution.
- Filled the previously empty readme and fixed a typo in the crate description.

## 1.1.0
- Fatal errors are visible by default. The terminal layer now always installs,
  writing to stderr unless `--log-to-console` selects stdout, so a tool that only
  calls `error!` no longer needs a channel flag for its errors to appear.
- stdout stays a pure data channel; diagnostics default to stderr. The stderr
  format drops timestamps and suppresses ANSI when stderr is not a terminal.
- `--log-to-file` keeps the file layer, and the terminal layer stays active
  alongside it so a fatal error is never confined to the log file.
- File-open failures emit one warning naming the path and cause, from
  `file_layer()`, covering both the standard and OpenTelemetry init paths.
- Default `--log-level` changed from `error` to `warn`, so warnings surface
  without an extra flag.
- `-L disabled` and the internal force-disable flag now short-circuit before any
  layer is built, so `-L disabled` suppresses all output unconditionally and
  overrides `RUST_LOG`. Previously it produced no output only in the common case:
  combined with a channel flag and a set `RUST_LOG`, the old path still installed
  a subscriber and could emit. Every other level continues to yield to `RUST_LOG`.

## 1.0.0
- Initial release: common CLI argument parser and basic logging shared across
  the tools.
