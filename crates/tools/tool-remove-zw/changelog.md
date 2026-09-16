# Changelog

## 2.2.0
- Add `--fail-on-skip` (valid only with `--check`): exits 1 when any input was skipped
  (binary, UTF-16/32, extension filter), so a gate cannot report a clean tree it never
  actually verified. Without the flag, skips stay neutral as before.
- The `Inputs:`/`Output:` header group lines render through
  `common_cli::header_format::format_config_label`; output bytes are unchanged.

## 2.1.0
- Add `--check`: scans like `--dry-run` (writes nothing, same report lines, a `Check:` summary) and exits 0 when nothing would change, 1 when at least one input would be modified, and 2 when the run fails, so the tool works as a pipeline quality gate. Conflicts with `--in-place`, `--output`, and `--dry-run`.
- Add `--keep-bom`: keeps a leading UTF-8 BOM instead of stripping it; a kept BOM does not count as a change, so a file whose only issue is the BOM is reported clean. Mid-stream U+FEFF characters are still removed.
- Validation failures (for example a missing input file) now print `Invalid arguments: ...` to stderr; previously they exited silently. Under `--check` they exit 2 instead of 1, so a typo'd path reads as a tool error rather than a dirty tree.
- Readme: corrected the runtime-header note (the header is opt-in via `--app-header` and prints to stdout; the old line still described the 1.1.0 always-on stderr header), listed the shared common-cli flags, and documented the short aliases `-o`, `-r`, `-d`, `-e`.

## 2.0.1
- The `label: value` header lines are rendered through `common-cli`'s `header_format` helpers; output bytes are unchanged.
- Reading stdin now goes through a small `Read`-based seam so the stdin path is covered by tests; behavior is identical.
- Test coverage extended: direct unit tests for the extension filter and the on-disk file classifier, tests for the stdin path (valid and invalid UTF-8), and end-to-end tests over the previously unused fixtures in `test-files/` (each `cf_*.txt` and `complex.txt` cleans, `no_cf.txt` stays untouched).

## 2.0.0
- Refactored to fit the new tooling: CLI parsing now uses the `clap` derive API and the shared `CommonToolArgs`, replacing the hand-built `Command` from the retired `shared` crate.
- Dropped the `-n/--no-header` flag. The runtime configuration is now shown on demand with the shared `--app-header` flag, which prints to stdout.
- Gained the shared runtime flags: `--log-level` (case insensitive), `--app-header`, `--log-to-console`, `--log-to-file`, and `--rotate-log-file-by-day`.
- Verbose reporting now uses the shared `--verbose` flag (the previous short alias is gone).
- Exit codes are explicit: `0` on success and `1` on failure.

## 1.1.0
- Strip a leading UTF-8 byte-order mark (BOM) from text files, counted separately from zero-width characters.
- Add `--dry-run` / `-d`: report which files would be modified and how, without writing anything.
- Fix `--output FILE`: the file is now written even when the input is already clean (previously nothing was created).
- Fix `--in-place`: it now works on directory inputs, and for stdin it is a no-op that writes to stdout, as its help already claimed.
- Skip UTF-16 / UTF-32 files with a clear reason instead of a generic "binary" skip (transcoding them is out of scope).
- Harden in-place writes: rename over the original without deleting it first, and carry the original file's permissions onto the replacement.
- Print the runtime header to stderr so stdout holds only the cleaned content or the dry-run report.
- Spell out in the header and `--help` that with no path it reads standard input and works as a filter, so the wait for input is expected rather than confusing.

## 1.0.0
- Initial release of the `remove-zw` tool.
- Removes all Unicode format (Cf) characters from stdin, files, or directories.
- Non-destructive by default (writes `<stem>.cleaned<ext>`); supports `--in-place`, `--output`, `--recursive`, and `--extensions`.