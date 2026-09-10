# Changelog

## 2.0.1
- BREAKING: unparseable datetime input now exits with code 1 and reports the
  failure on stderr. It printed "Invalid date-time format" to stdout and exited
  0, so scripts could not tell a failed parse from a conversion.
- Conversion output is built by string-returning functions
  (`format_unix_to_datetime`, `datetime_to_unix`) with printing kept in thin
  wrappers; tests pin the epoch UTC line byte-for-byte and pin that a
  milliseconds input yields the same datetime as its seconds value.
- The numeric-input classification (seconds vs milliseconds at more than 10
  characters) moved into a testable `interpret_numeric` function. Known edge
  recorded in the readme: an 11-character negative seconds value is read as
  milliseconds.
- Fixed the `print_current_unix_timestamp` doc: the timestamp is
  timezone-independent, not "local time".
- Readme: the `--app-header` example block now matches the real output: `Verbose mode: <unused>`,
  the `Tool Runtime Config` section line before the Input item, and the trailing blank line.
- The header line is rendered through `common-cli`'s `header_format` helper;
  output bytes are unchanged.
- Readme rewritten: real output (no emoji header, header only under
  `--app-header`, current version), shared flags documented, error contract
  documented.

## 2.0.0
- Refactored to fit the new tooling: CLI parsing now uses the `clap` derive API and the shared `CommonToolArgs`, replacing the hand-built `Command` from the retired `shared` crate.
- The runtime configuration header is now shown on demand with the shared `--app-header` flag (previously printed on every run).
- Gained the shared runtime flags: `--log-level` (case insensitive, long form only), `--app-header`, `--log-to-console`, `--log-to-file`, and `--rotate-log-file-by-day`.
- Logging is now installed through the shared `AppLogger` during boot-up, and exit codes are explicit: `0` on success and `1` on failure.

## 1.1.2
- Updated dependencies.

## 1.1.1
- Removed emojis. They don't render properly on every terminal.

## 1.1.0
- Now able to process numeric timestamps with milliseconds alongside the traditional unix timestamp.

## 1.0.0
- Initial release.