# Changelog

## 2.0.4
- The single-guid print goes through `common_cli::broken_pipe`: `guid | true` exits 0 with a
  debug note instead of panicking with 101 (the bulk `-m` path already behaved this way). A
  requested clipboard copy still happens when the pipe is closed, since it does not depend on
  stdout. New process-level tests pin both output paths against a pre-closed pipe.

## 2.0.3
- Test suites use the shared `common_cli::test_writers` doubles instead of the two private
  copies (a closed-pipe writer and a failing-disk writer); behavior pinned by the tests is
  unchanged.

## 2.0.2
- `guid -m N | head` exits 0 quietly when the consumer closes the pipe, instead of reporting
  "Error while writing guids" and exiting 1. Writes go through `common-cli`'s broken-pipe
  helpers, so any other write failure still exits 1, with the message now reading
  "failed to write output: ...".
- Error handling moved to the entrypoint per CONTRIBUTING.md: `guid_app.rs` returns errors
  instead of calling `exit_error()` itself, and the clipboard helper takes `&str` and returns a
  `Result` that main reports.

## 2.0.1
- `-m/--multiple` prints one guid per line. It printed each guid followed by a
  carriage return, so on a terminal every guid overwrote the previous one and
  only the last was visible.
- The single-guid path prints a complete line (trailing newline). Buffered
  output is no longer lost on exit: common-cli 1.3.0 flushes stdout in the exit
  helpers.
- The zero-count check moved into a testable `validate_multiple` function and
  reports on stderr before logging is installed.
- Removed a dead error-handling arm in `main` and rewrote doc comments that
  still described removed interval/silence flags.
- First tests for the crate: guid creation, per-line multiple output, count
  validation, and a clap definition check.
- Runtime-config lines are rendered through `common-cli`'s `header_format`
  helpers; output bytes are unchanged.
- Readme rewritten to document the real flags (`-m`, `-c`, `-e` and the shared
  ones); it described interval/silent flags that do not exist and claimed the
  tool cannot generate multiple UUIDs.
- Note: the 2.0.0 entry below shipped without bumping Cargo.toml (it stayed
  1.0.2); this release syncs the crate version.

## 2.0.0
- Refactored to fit the new tooling.

## 1.0.2
- Updated dependencies.

## 1.0.1
- Removed emojis. They don't render properly on every terminal.

## 1.0.0
Initial release