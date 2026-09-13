# Changelog

## 2.2.0
- `-d` accepts the offset-aware T-separated ISO forms: `2024-01-15T10:30:45+0900`, with a
  `.`/`,` fraction, a `+09:00` colon offset, or a space before the offset. Only the
  space-separated `%z` forms parsed offset-aware before.
- The three production `current_times.unwrap()` calls are gone: the final timestamp pair is
  resolved in one place that reads the file's current times only for partial updates, where
  the untouched timestamp must keep its value. Side effect: a full update (both timestamps,
  or no explicit source with both flags) no longer performs a redundant metadata read, so
  that failure mode disappears there.

## 2.1.0
- `-d` accepts the POSIX form `YYYY-MM-DDThh:mm:SS[.frac][Z]`: the `T` separator (a space still works), fractional seconds with `.` or `,` (carried into the file timestamp at nanosecond precision), and a trailing `Z` meaning UTC.
- Offset-carrying date strings (`2024-01-15 10:30:45 +0900`, RFC-2822 style) now honor their offset. They used to be read as local wall time with the offset silently discarded.
- Fixed the `-t` century pivot to POSIX: two-digit years 69-99 resolve to 19xx (`-t 69...` used to become 2069); 00-68 stay 20xx.
- The `-d` parse error now lists the accepted forms.
- Fixed silent argument errors: logging booted only after validation, so an invalid `-d`/`-t`/`--time` value exited 1 with no message (the entrypoint's `error!` had no subscriber). Logging now boots first and the error prints to stderr.
- Decision recorded: touch keeps the original's month-first `MM/DD/YYYY` reading (the GNU US convention); tool-timestamp's day-first parsing is that tool's own convention and stays. GNU relative items (`yesterday`, `2 days ago`) remain unsupported.
- Readme: documented the accepted-and-ignored `-f` compatibility flag.

## 2.0.1
- Fixed a panic on multibyte `-t` values: the parser sliced the input by byte index, so a value of 8, 10, or 12 bytes containing a multibyte character crashed on a char boundary. The parser now rejects anything other than digits and the optional dot with an error.
- Fixed `-d` with date-only values (`2024-01-15`, `01/15/2024`, `15 Jan 2024`): the formats were listed but never matched because the parser demanded a time component, so these inputs always errored. They now resolve to midnight local time.
- Added unit tests for `-t` parsing (all three lengths, seconds suffix, length and seconds errors, multibyte rejection), `-d` parsing (every supported format, `now`, garbage), and `TouchArgs::get_current_filetime` precedence (`-d` over `-t` over `-r`, `None` when unset).
- Filesystem tests now pin file timestamps to a fixed past instant before running the tool, so access-time assertions no longer race against other processes reading the freshly created file.
- Readme corrections: removed the per-file "Result" output blocks (the tool prints nothing on success) and documented the shared runtime flags.

## 2.0.0
- Refactored to fit the new tooling: CLI parsing now uses the `clap` derive API and the shared `CommonToolArgs`, replacing the hand-built `Command` from the retired `shared` crate.
- Gained the shared runtime flags: `--log-level` (long-only, case insensitive), `--app-header`, `--log-to-console`, `--log-to-file`, and `--rotate-log-file-by-day`.
- Argument errors (invalid `--date`, `-t`, `--reference`, `--time`, or more than one time source) now surface through the shared logging pipeline; exit codes are explicit: `0` on success and `1` on failure.

## 1.0.1
- Updated dependencies.

## 1.0.0
- Initial release.