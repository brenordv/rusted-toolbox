# Changelog

## 1.7.0
- Added the `test_writers` module: shared failing `Write` doubles for the tools' test suites
  (`ClosedPipe`, `FailingDisk`, `FailingFlush`, and the byte-budget `FailAfter` with a
  configurable error kind). Several crates carried private copies of these shapes; they now
  import the shared ones.
- The `--app-header` block is broken-pipe safe: the standard header, the "Tool Runtime Config"
  section line, and the trailing blank line go through `broken_pipe::write_out`, so a consumer
  that closes stdout before boot gets a debug note instead of a panic. On a dead pipe the rest
  of the block is skipped, the tool's own printer callback included (it would hit the same
  pipe); any other stdout write error logs one warning naming the error and skips the same
  way. Boot never fails because of stdout state, and the happy-path bytes are unchanged
  (pinned by new tests). Closes the fleet-wide gap recorded in the workspace backlog
  (csv-split 3.0.2's note).
- New `tracing` dependency for the two diagnostics above.

## 1.6.0
- Added `header_format::format_config_label`: the label-only level-2 line renderer for group or
  mode lines that carry no value (`Inputs:`, `Resolved Target`). pingx and remove-zw adopted it
  in place of hand-assembled `CONFIG_UL_ITEM_LEVEL_2` prints; output bytes are unchanged.

## 1.5.0
- Added the `broken_pipe` module: a `BrokenPipe` marker type plus `write_out`/`flush_out` wrappers
  that map a closed-pipe write failure into the marker inside the `anyhow` chain, so a streaming
  tool can end quietly with exit 0 when its consumer stops reading (`tool big.txt | head`). First
  consumers: head, tail, and rxget; cat, b64, and split still carry local copies of the pattern
  and can adopt it later.

## 1.4.1
- `--log-to-file` now restricts log-file permissions on Unix: the logs directory is created 0o700
  and the file is opened 0o600. A file that already exists is tightened to owner-only through the
  open handle on the next run, so logs created wide by earlier versions heal themselves. If the
  tighten fails, file logging is skipped for that run with the usual stderr warning rather than
  writing sensitive content into a file that stayed readable. Execution logs can carry response
  bodies, which is what makes the mode worth enforcing.
- Covers every active tool that logs through `AppLogger::file_layer`, on both the standard and
  OpenTelemetry init paths. On Windows the log keeps inheriting the parent directory's ACL (the
  user profile); there is no mode-bit equivalent there.

## 1.4.0
- Added `CommonToolArgsNoVerbose`, a second flatten shape for tools that own
  their verbosity flag: the shared flags minus `--verbose`, with `--log-level`
  as an `Option` so "not passed" stays distinguishable. `resolved_level` picks
  the explicit flag over the tool's derived default, and
  `app_boot_up_with_level` boots logging and the `--app-header` block from that
  resolved level. First consumer: whurl, whose `-v` count flag would collide
  with the shared bool `--verbose`.
- Both boot paths print the header block through one private helper, and a test
  pins the shared flag definitions in sync across the two structs, so neither
  the bytes nor the flags can drift.

## 1.3.0
- The `header_format` module is now public: `format_config_section` and
  `format_config_item` are exported, and a new `format_config_item_level3`
  covers the nested detail lines. Tool header printers can build their sections
  from these renderers instead of hand-assembling lines from the `CONFIG_UL_*`
  constants (which stay public and unchanged). Output bytes are pinned by tests.
- `exit_success`, `exit_error`, and `exit_with_code` flush stdout and stderr
  before `std::process::exit`, so output printed just before an exit helper is
  no longer lost to the skipped `Drop` of Rust's buffered stdout. They still do
  not run `Drop` implementations.

## 1.2.1
- The `--app-header` block is now rendered by a private pure formatter
  (`header_format`) whose tests pin the exact output bytes; `app_boot_up`
  prints the rendered string, and its output is byte-identical to before. This
  resolves the old TODO about the hand-assembled CONFIG_UL printing.
- Added clap parse tests for the flattened `CommonToolArgs` (defaults,
  case-insensitive `--log-level`).
- Readme gained a "Logging contract" section.

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
