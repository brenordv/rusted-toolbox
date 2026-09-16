# remove-zw

`remove-zw` removes zero-width Unicode format (Cf) characters from text and strips a leading
byte-order mark (BOM) from UTF-8 files. It works with stdin and files, and defaults to
non-destructive output when file inputs are provided.

## Command Line Examples
### Read from stdin, write to stdout
The input contains a zero-width space (U+200B) between the two words:
```bash
$ printf 'hello​world\n' | remove-zw
helloworld
```

### Clean a file (writes to a new file)
```bash
$ remove-zw notes.txt
```

### Clean a directory (non-recursive)
```bash
$ remove-zw ./docs
```

### Clean a directory recursively
```bash
$ remove-zw --recursive ./docs
```

### Limit to specific extensions
```bash
$ remove-zw --recursive --extensions txt,md,rs ./docs
```

### Overwrite files in place
```bash
$ remove-zw --in-place notes.txt
$ remove-zw --in-place --recursive ./docs
```

### Preview changes without writing (dry run)
```bash
$ remove-zw --dry-run --recursive ./docs
would modify: docs/a.txt (BOM removed, 3 zero-width removed)
unchanged: docs/b.txt
skipped: docs/logo.png (binary file)
skipped: docs/utf16.txt (UTF-16/UTF-32 not supported)
Dry run: 1 would modify, 1 unchanged, 2 skipped.
```

### Keep a leading BOM while removing zero-width characters
```bash
$ remove-zw --keep-bom --in-place notes.txt
```

## Check mode (pipelines and CI)
`--check` scans exactly like `--dry-run` (same report lines, nothing written) and turns the
result into an exit code, so the tool works as a quality gate:

```bash
$ remove-zw --check --recursive ./docs
would modify: docs/a.txt (BOM removed, 3 zero-width removed)
unchanged: docs/b.txt
skipped: docs/logo.png (binary file)
Check: 1 would modify, 1 unchanged, 1 skipped.
$ echo $?
1
```

Exit codes with `--check`:

| Code | Meaning |
|------|---------|
| 0 | Run completed; nothing would be modified |
| 1 | Run completed; at least one input would be modified (or, with `--fail-on-skip`, at least one input was skipped) |
| 2 | The run failed: unreadable input, invalid arguments, stdin not valid UTF-8 |

Notes on the contract:
- The 0/1/2 contract applies to `--check` runs. Without `--check` the tool keeps its standard
  exit codes: 0 on success, 1 on any failure. Usage errors rejected by the argument parser
  (for example `--check --in-place`) exit 2 in every mode.
- Exits 0 and 1 always end with the `Check: ...` summary line; exit 2 never does, so a CI log
  can tell a gate verdict from a tool error at a glance.
- Skipped files (binary, extension-filtered, UTF-16/32) are unverified and by default never
  make the result dirty: a run whose inputs are all skipped, or that matches zero files,
  exits 0. Pass `--fail-on-skip` (valid only with `--check`) to exit 1 when anything was
  skipped, so a gate cannot report a clean tree it never actually verified. Symlinks inside a
  scanned directory are not followed and not reported. Gate authors should sanity-check the
  summary counts either way.
- A gate meant to reject BOMs must not pass `--keep-bom`: with the flag, a file whose only
  issue is a leading BOM exits 0 while still carrying its BOM.
- `--check` conflicts with `--in-place`, `--output`, and `--dry-run`. It works with stdin:
  `cat file | remove-zw --check` reports instead of cleaning. `--fail-on-skip` without
  `--check` is a usage error (exit 2).
- The run aborts at the first I/O error (exit 2); inputs after the failing one are unscanned.

### Force stdout for file inputs
```bash
$ remove-zw --output - notes.txt
```

### Verbose output
```bash
$ remove-zw --verbose notes.txt
```

## Notes
- With no file or directory argument (or with `-`), the tool reads standard input and works as a filter,
  so `... | remove-zw` and `remove-zw < file` work. Run in a terminal with no path, it waits for you to
  type input; end it with Ctrl+Z then Enter (Ctrl+D on Unix), or pass a path such as `remove-zw ./docs`.
  `--help` notes this so the wait is expected.
- By default, file inputs write to a new file named `<stem>.cleaned<ext>` (or `<stem>.cleaned`).
- Use `--output` to force stdout or a specific output file. An explicit `--output` target is written
  even when the input is already clean.
- Use `--in-place` to overwrite the original files. It applies to file and directory inputs; for stdin
  it is a no-op that writes to stdout.
- Use `--dry-run` to see what would change (BOM and zero-width counts per file, plus which files are
  skipped) without touching anything.
- A leading BOM is stripped from UTF-8 files unless `--keep-bom` is given; a kept BOM does not count
  as a change. A mid-stream U+FEFF is treated as a zero-width character, not a BOM, and is removed
  either way.
- UTF-16 and UTF-32 files are detected by their BOM and skipped; only UTF-8 text is processed.
- Directory inputs only process non-binary text files. Use `--extensions` to bypass binary detection.
- Stdout carries only the cleaned content or the report. The runtime header is opt-in: pass the shared
  `--app-header` flag to print it (tool name, version, inputs, and output mode) to stdout before the run.
- Shared flags from the common CLI: `--app-header`, `--verbose`, `--log-level <level>` (case
  insensitive), `--log-to-console`, `--log-to-file`, `--rotate-log-file-by-day`.
- `--output`, `--recursive`, `--dry-run`, and `--extensions` have short forms: `-o`, `-r`, `-d`, `-e`.
- In-place writes go through a temporary file that is renamed over the original, so a hard-linked file
  becomes its own copy and a symlink passed directly as an argument is replaced by a regular file.
