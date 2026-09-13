# Changelog

## 3.2.1
- The binary sniff (`is_probably_binary`) moved to `common-file-utils` so other tools can use
  it; the `text` search now calls it there. No behavior change: same 8 KiB NUL-byte sample,
  same skip counting and debug logging.

## 3.2.0
- Both modes now write their summary line to stderr. `files` mode used to put it on stdout,
  so `lookup files "*.log" | xargs ...` fed the summary into the consumer while `text` mode
  kept it on stderr; stdout now carries only matches in both modes. A scripted consumer that
  scraped the files-mode summary can recover the match count with `wc -l` on stdout.
- The missing-path error in `text` mode is reported once: the app returns it and main logs
  it, instead of both logging and returning it.

## 3.1.1
- Fixed `files` mode rejecting every invocation that relied on the default pattern mode:
  clap renders the `--file-search-pattern` default through `PatternMode`'s Display, which
  produced "Wildcard" while the accepted values are lowercase, so `lookup files "*.rs"`
  failed to parse with "invalid value 'Wildcard'". Display now emits the kebab-case value
  names; a test pins the defaulted parse.
- `files` traversal errors no longer print to stdout: they are warn-level log events on stderr,
  so `lookup files "*.log" | xargs ...` never feeds "Access is denied" lines into the consumer.
  The lines are log-formatted (level prefix) rather than the old bare `error: path` text and
  follow the shared log flags (`--log-level`, `--log-to-console`, `--log-to-file`); `--no-errors`
  still suppresses them entirely.
- Match output no longer aborts with exit 101 when the consumer closes the pipe:
  `lookup text error -p logs | head -5` now ends the run quietly with exit 0 (via
  `common-cli`'s broken-pipe handling, as in guid and get-lines). Both search modes and the
  `files` summary line write through the same seam, so a closed pipe at any point exits clean
  while other write failures still report and exit 1.

## 3.1.0
- The `text` search now detects binary files (a NUL byte within the first 8 KiB) and skips them; each skip is counted and logged at debug level via the new `lookup_shared::is_probably_binary` helper.
- The `text` search counts lines skipped for invalid UTF-8 instead of dropping them silently.
- The `text` summary line now also reports binary files skipped and invalid UTF-8 lines skipped; `run_text_lookup` returns all counters in a `TextLookupCounts` struct so tests can assert results.
- Fixed a clap misconfiguration: the `text` positional carried `required_unless_present = "text"` referencing a nonexistent argument id; the positional is now plainly required. A `debug_assert` test guards the CLI definition.
- Fixed the `files` help examples advertising a nonexistent `--regex` flag; both subcommands now carry the readme's real examples in their `after_help` (resolving the two TODO markers).
- Fixed the missing bullet prefix on the "Print Line data only" line of the `text` header; both headers now render through `common-cli`'s `header_format::format_config_item`, with output bytes otherwise unchanged. The direct `common-utils` dependency became unused and was dropped.
- Readme rewritten to match the real CLI: removed flags that do not exist (`-t/--text`, `--no-header`, `--regex`, `--wildcard`, and `files`' `-c/--current-only`), documented `-s/--file-search-pattern`, `-n/--no-recursive`, `-m/--no-summary`, the binary-skip behavior, and the new summary counts.
- Added tests: clap debug assertions, matcher engine (wildcard/regex, case sensitivity, RegexList fallback arm), `is_probably_binary`, and end-to-end binary-skip and invalid-UTF-8 counting over temp dirs.

## 3.0.0
- Refactored to fit the new tooling.

## 2.0.0
- Introduced subcommands: `text` and `files`.
  - `text` retains previous behavior and adds a concise positional form: `lookup text "your text"`.
  - Prior flags like `--text/-t`, `--extension/-e`, `--path/-p`, `--current-only/-c`, `--line-only/-l`, and `--no-header` remain supported under the `text` subcommand.
- New `files` subcommand to search for files by filename.
  - Supports wildcard (default) or regex patterns; case-insensitive by default.
  - Recursive by default; `--current-only` limits search to the current directory.
  - Progress displays the folder being read on a single updating line; line is cleared before updates to avoid overlapping.
  - On errors while traversing, clears the progress line, then prints a brief error message and continues. Errors can be suppressed with `--no-errors`.
  - On match, prints the absolute path (Windows verbatim prefixes like `\\?\` stripped for readability).
  - Final summary printed at the end (dirs, files, matches, elapsed); can be suppressed with `--no-summary`.
  - Other controls: `--no-progress`, `--no-header`, `--regex`, `--wildcard`, `--case-sensitive`.
- CLI UX improvement: if no subcommand is provided, the app now prints usage/help and exits.
- Help examples updated, including a regex that matches `mydoc.pdf`, `mydoc.epub`, or `mydoc.mobi`: `lookup files --regex "^mydoc\.(pdf|epub|mobi)$"`.
- Internal refactor: shared, tool-specific helpers extracted to `lookup_shared.rs`; `lookup_text_app.rs` and `lookup_files_app.rs` contain subcommand-specific logic.
- Dependencies added for filename search: `regex`, `globset`, `walkdir`.

## 1.0.1
- Added support for matching by exact file name for dotfiles or bare names.
  - Pattern ".env" matches basename ".env"
  - Pattern "env" matches basename "env"

## 1.0.0
- Initial release.