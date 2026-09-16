# rxget
## What It Is
`rxget` extracts values from text files using a regex and prints them, one per line. It is
narrower than grep on purpose: it does one job (pattern-extract values across one or more
files, with optional uniqueness and optional filename prefixes) and is built to do that job
fast on large inputs.

The value is capture group 1 when the pattern has capture groups, the whole match otherwise.
Every non-overlapping match on every line counts.

## Examples
### Extract every ID from a log
```bash
rxget -p 'id=(\d+)' app.log
```

### Each distinct value once per file
```bash
rxget -p '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' -m unique-per-file logs/*.log
```

### Each distinct value once across the whole run, with the source file
```bash
rxget -p 'user=(\w+)' -m unique-per-run -H logs/*.log
```

### Wildcards on Windows
The tool expands wildcards itself, so patterns work the same in shells that do not:
```bash
rxget -p "error code (\d+)" logs\*.log
rxget -p "error code (\d+)" "logs/**/*.log"
```

### Read standard input
A `-` target reads standard input; it mixes with files and patterns at its position:
```bash
journalctl -u app | rxget -p 'id=(\d+)' -
rxget -p 'user=(\w+)' -m unique-per-run before.log - after.log
```

## Command-line options
| Flag                      | Description                                                                                                                    |
|---------------------------|--------------------------------------------------------------------------------------------------------------------------------|
| `<TARGET>...`             | Files, wildcard patterns, and/or `-` for standard input (at least one). Supports `*`, `?`, `[ab]`, `{a,b}`, and recursive `**` |
| `-p`, `--pattern <REGEX>` | Extraction regex (required); group 1 is the value when the pattern has capture groups                                          |
| `-m`, `--mode <MODE>`     | `all` (default), `unique-per-file`, or `unique-per-run`; unique modes print the first occurrence and suppress later duplicates |
| `-H`, `--with-filename`   | Prefix each value with the input it came from, as `<name>: <value>` (`standard input` for `-`)                                 |

The shared workspace flags (`--app-header`, `--log-level`, `--log-to-console`, `--log-to-file`,
`--rotate-log-file-by-day`, `--verbose`) are accepted; `--verbose` is unused by this tool.
Regex case sensitivity is the pattern's business: use `(?i)` for case-insensitive matching.

## Exit codes
| Code | Meaning                                                                                                                                                           |
|------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| 0    | Run completed, even when zero values were extracted (rxget is an extractor, not a matcher; grep's exit-1-on-no-match contract is deliberately not copied)         |
| 1    | Any target error (missing file, directory argument, malformed pattern argument, empty expansion), a per-file read failure, an over-long line, or an invalid regex |
| 130  | Interrupted with Ctrl+C; interruption wins over an earlier per-file failure                                                                                       |

## Notes and caveats
- Matching is per line; the pattern cannot match across line boundaries.
- A single line longer than 8 MiB with no newline is a per-file error: the rest of that file is
  skipped (values there are never scanned), the run continues with the next file and ends with
  exit 1.
- Target expansion: an existing file is used as-is; a directory argument is an error (pass a
  pattern like `dir/*.log` instead); `*` and `?` do not cross path separators; `**` recurses.
  Glob matching is case-insensitive on Windows and case-sensitive elsewhere. On Windows a
  backslash in a pattern is a path separator; on Unix it escapes the next character.
- Symlinks to files match like shell globs; symlinked directories are not walked during
  expansion.
- Glob expansion matches dotfiles: `*.log` also matches `.secret.log`, unlike Unix shells.
- Deduplication of targets is lexical, by parsed path components, first occurrence kept:
  spellings that differ only in separators or a leading `./` count once, while the same file
  reached through a symlink or `..` traversal is processed twice.
- Unique modes hold every distinct value in memory; a run over unbounded distinct values grows
  accordingly.
- Values and `-H` path prefixes print as raw bytes. Content or names containing terminal escape
  sequences render as such on a console, and a name containing `: ` can visually forge the
  `path: value` shape; piping to a file is the normal use for consumers that parse the output.
- The regex engine guarantees linear-time matching (no catastrophic backtracking); a huge
  pattern against long lines is slow but never exponential, and Ctrl+C takes effect at the next
  line boundary, not inside a single match call.
- `--log-level debug` prints an end-of-run summary: targets processed, values emitted,
  duplicates suppressed.
- Standard input: the exact argument `-` reads stdin at its position in the target order;
  repeated `-` arguments read it once. It labels as `standard input` in `-H` prefixes and
  diagnostics, and `unique-per-file` treats the whole stream as one file. A real file named
  `-` stays reachable as `./-`. The 8 MiB line cap applies to stdin too. Ctrl+C is observed
  between lines, so at an idle interactive stdin it takes effect on the next line or EOF.