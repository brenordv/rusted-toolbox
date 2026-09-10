# Changelog

## 2.0.1
- BREAKING: a read error mid-split (for example non-UTF-8 input) now stops the
  run with exit code 1. It logged the error, stopped reading, and exited 0, so
  a silently truncated split looked like a success.
- BREAKING: interrupting a run with Ctrl+C now exits with code 130 (matching
  csvn and get-lines) instead of 0, and the line already read at that moment is
  written before stopping instead of being dropped.
- Progress feedback no longer panics when stdout is closed (for example when
  piped into a command that exits early); it warns once and disables further
  feedback. The cadence check uses integer arithmetic instead of float modulo.
- `--feedback-interval 0` is rejected (it silently disabled feedback through a
  NaN comparison).
- Startup failures are reported with `eprintln!`; they were logged through
  `error!` before the tracing subscriber existed and vanished.
- `build_args` takes an explicit base directory, with tests for relative-path
  resolution, the output-dir default, and absolute passthrough; added tests for
  CRLF input, read-error propagation, the interrupt outcome, and the feedback
  format.
- Header lines are rendered through `common-cli`'s `header_format` helpers;
  output bytes are unchanged.
- Readme gained a command-line options section and documents the exit-code and
  UTF-8 contracts.

## 2.0.0
- Refactored to fit the new tooling: CLI parsing now uses the `clap` derive API and the shared `CommonToolArgs`, replacing the hand-built `Command` from the retired `shared` crate.
- The runtime configuration header is now shown on demand with the shared `--app-header` flag (previously printed on every run).
- Gained the shared runtime flags: `--log-level` (case insensitive, long form only), `--app-header`, `--log-to-console`, `--log-to-file`, and `--rotate-log-file-by-day`.
- Graceful shutdown, the datetime/format helpers, and size constants now come from the shared `cli-signal-monitor` and `common-utils` crates.
- Exit codes are explicit: `0` on success and `1` on failure.

## 1.0.2
- Updated dependencies.

## 1.0.1
- Removed emojis. They don't render properly on every terminal.

## 1.0.0
Initial release