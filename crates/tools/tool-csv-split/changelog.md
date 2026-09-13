# Changelog

## 3.0.2
- The startup config banner's own lines (the six items under "Tool Runtime Config") go
  through `common_cli::broken_pipe` now: a consumer that closes stdout before they print
  produces a debug note instead of a panic, and any other stdout failure a warning; the run
  itself is never failed by the banner. The standard header block around those lines still
  prints through common-cli's shared boot path, which is not yet broken-pipe safe; that gap
  is recorded in the workspace backlog.

## 3.0.1
- A failed flush of an output part stops the run with exit code 1; it logged a warning
  and exited 0, so an incomplete part file on disk looked like a success. The error names
  the part file. This closes the known limit recorded under 3.0.0. The fix is about the
  exit code, not durability: parts are flushed, not fsynced, same as the rest of the
  fleet.
- The remaining bare `println!` calls in the split loop go through
  `common_cli::broken_pipe` now: the final elapsed-time pair, the Ctrl+C "saving
  progress" notice, and the CSV-header notice. `csv-split -f big.csv | head -1` exits 0
  instead of panicking with exit 101 after a successful split. The startup config banner
  still prints directly; it runs before any long output, so a consumer closing the pipe
  mid-run never reaches it.

## 3.0.0
- BREAKING: the tool is now `csv-split` (crate `csv-split`, directory
  `crates/tools/tool-csv-split`). The old name shadowed coreutils `split` while doing a
  different, CSV-focused job; the rename keeps the tool out of the Unix ported-tools
  exclusion list by construction. Scripts calling `split` expecting this tool must switch
  to `csv-split`.
- Logging boots before validation, so a validation failure (missing input file, zero
  `--lines-per-file`) is reported through the subscriber; main's failure arm uses `error!`
  like the sibling tools instead of a pre-boot stderr print.
- "CSV mode enabled but no header line found" logs at `warn!` (the run continues and
  produces headerless parts); it was an `error!` with a "Warning:" prefix.
- Flush failures are `warn!` events now (they went through bare `eprintln!`), so they reach
  `--log-to-file` too, and the final-flush warning names the output file. Known limit,
  recorded in next.md: a failed flush still exits 0 although the part file on disk may be
  incomplete.

## 2.0.2
- Progress feedback goes through `common_cli::broken_pipe` now: when stdout is closed by the
  consumer, feedback stops with a debug note instead of the warning reserved for real write
  failures. Chunk writes still go to files only; the split itself is unaffected either way.

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