# 2.0.0
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

# 1.0.2
- Updated dependencies.

# 1.0.1
- Removed emojis. They don't render properly on every terminal.

# 1.0.0
- Initial release.