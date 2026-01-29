# remove-zw

`remove-zw` removes zero-width Unicode format (Cf) characters from text. It works with stdin
and files, and defaults to non-destructive output when file inputs are provided.

## Command Line Examples
### Read from stdin, write to stdout
```bash
$ echo "hello\u200Bworld" | remove-zw
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

### Overwrite a file in place
```bash
$ remove-zw --in-place notes.txt
```

### Force stdout for file inputs
```bash
$ remove-zw --output - notes.txt
```

### Verbose output and no header
```bash
$ remove-zw --verbose --no-header notes.txt
```

## Notes
- By default, file inputs write to a new file named `<stem>.cleaned<ext>` (or `<stem>.cleaned`).
- Use `--output` to force stdout or a specific output file.
- Use `--in-place` to overwrite the original files.
- Directory inputs only process non-binary text files. Use `--extensions` to avoid binary detection.
