# Changelog

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