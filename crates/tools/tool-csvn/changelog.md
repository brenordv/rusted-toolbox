# Changelog

## 2.0.2
- An I/O-class error while reading records (for example a network share dropping mid-run) now
  aborts the run with an error naming the input file and the rows read so far. Previously the
  error arm counted the row as skipped and retried the same position forever, spamming
  "Skipping unparseable row". Parse-class errors (such as invalid UTF-8) still skip the
  offending row and continue. The partial `_normalized` file is kept, same as an interrupted
  run.
- The progress line no longer shows "NaN lines/s" when no measurable time has elapsed (for
  example on an empty input); it shows `0.00` instead.
- The "Skipping unparseable row" and "No default value mapped for column" warnings go through
  the logger instead of raw stderr writes, so they also reach `--log-to-file`. At the default
  level they still print to stderr; `--log-level error` or `disabled` now silences them, and
  `--log-to-console` routes them to stdout with the rest of the log stream.
- Run failures reported by the entrypoint now print the full error chain, so the underlying
  OS error (missing path, dropped share) is visible instead of only the outermost context.

## 2.0.1
- Pre-boot startup failures (missing input file, argument errors) are now written with `eprintln!`. The 2.0.0 entry claimed this fix, but the call site still used `error!`, which discards the message because the tracing subscriber is only installed later during boot-up.
- Removed a warning emoji left in the `--clean-string` runtime notice (emoji output was dropped in 1.0.1).
- Extracted header and value-map parsing out of `initialize()` into pure `parse_headers`/`parse_value_map` functions and added tests for them, for unparseable-row skipping, and for the warn-once empty-column path.
- Runtime-config lines are rendered through `common-cli`'s `header_format` helpers; output bytes are unchanged apart from the emoji removal.
- Readme: documented that `-e/--headers` makes the input read as headerless and that `--feedback-interval` has a minimum of 1.
- Readme: listed the shared common-cli flags and documented value-map parsing (keys trimmed and lowercased, values lose surrounding quotes, last duplicate key wins).

## 2.0.0
- Refactored to fit the new tooling.
- Rows whose column count differs from the header are now repaired instead of silently dropped: short rows are padded with defaults, long rows are truncated to the header width, and the repaired count is reported at the end.
- Custom headers (`-e/--headers`) now keep the first data row; the file is read as headerless when headers are supplied.
- Startup errors (missing file, invalid arguments) are printed to stderr instead of vanishing before logging is initialized.
- Interrupting a run with Ctrl-C now exits with code 130 so scripts can tell a partial output from a complete one.
- Output paths are derived with proper path handling, fixing extensionless files inside dotted directories and supporting non-UTF-8 paths.
- A default value given for a duplicated column name now maps to the correct column.
- A wildcard default (`*`) combined with column-specific defaults now applies the wildcard to every other column instead of being ignored.
- Replaced the memory-mapped reader with a buffered CSV reader (same throughput, no unsafe code) and dropped the string interner; per-column defaults are precomputed once and the row loop reuses records, making it allocation-free and O(columns) per row.
- Progress feedback is throttled to at most four updates per second, and `--feedback-interval` now rejects `0`.

## 1.0.2
- Updated dependencies.

## 1.0.1
- Removed emojis. They don't render properly on every terminal.

## 1.0.0
- Initial release.