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
  The header and `--help` note this so the wait is expected.
- By default, file inputs write to a new file named `<stem>.cleaned<ext>` (or `<stem>.cleaned`).
- Use `--output` to force stdout or a specific output file. An explicit `--output` target is written
  even when the input is already clean.
- Use `--in-place` to overwrite the original files. It applies to file and directory inputs; for stdin
  it is a no-op that writes to stdout.
- Use `--dry-run` to see what would change (BOM and zero-width counts per file, plus which files are
  skipped) without touching anything.
- A leading BOM is stripped from UTF-8 files. A mid-stream U+FEFF is treated as a zero-width character,
  not a BOM.
- UTF-16 and UTF-32 files are detected by their BOM and skipped; only UTF-8 text is processed.
- Directory inputs only process non-binary text files. Use `--extensions` to bypass binary detection.
- The runtime header prints to stderr, so stdout carries only the cleaned content or the dry-run report.
- In-place writes go through a temporary file that is renamed over the original, so a hard-linked file
  becomes its own copy and a symlink passed directly as an argument is replaced by a regular file.
