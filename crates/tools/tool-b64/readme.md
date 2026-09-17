# b64

`b64` is a flexible Base64 encoder/decoder for the terminal. It embraces familiar GNU-style behavior from [base64](https://www.gnu.org/software/coreutils/manual/html_node/base64-invocation.html), 
while adding helpful quality-of-life improvements:

- Encode or decode from files, stdin, or inline text.
- Automatic detection: `b64 input` treats `input` as a file when it exists, otherwise as literal text.
  Two edge cases: a path that exists but is a directory is treated as literal text, and a path whose
  metadata cannot be read because of a permission error is treated as a file, so the run reports the
  open failure instead of silently encoding the path string.
- Explicit modes: force file mode with `--file`/`-f`, or text mode with `--text`/`-t`.
- Optional line wrapping that matches the traditional 76-column layout (configurable with `--wrap`).
- `--ignore-garbage` keeps decoding resilient when pasted data includes whitespace or noisy characters.

## Command Line Examples
### Encode a File (auto-detected)
```bash
$ b64 ./logo.png
iVBORw0KGgoAAAANSUhEUgAA... (truncated)
```

### Decode to a File
```bash
$ b64 -d --file encoded.txt --output decoded.bin
```

### Encode Inline Text
```bash
$ b64 --text "hello world"
aGVsbG8gd29ybGQ=
```

### Decode noisy input with garbage filtering
```bash
$ printf ' aGVs\nbG8gd29ybGQ=\t' | b64 -d --ignore-garbage
hello world
```
`printf` expands `\n` and `\t` into a real newline and tab, so the stream carries genuine
whitespace noise that `--ignore-garbage` filters out. Inline text works the same way when the
noise is plain spaces: `b64 -d --text " aGVs bG8gd29ybGQ= " --ignore-garbage`.

### Disable Line Wrapping
```bash
$ b64 --wrap 0 --text "toolbox"
dG9vbGJveA==
```

### Read from stdin, write to stdout
```bash
$ echo "raccoon.ninja" | b64
cmFjY29vbi5uaW5qYQ==
```

## Flags
Every option has a short form: `-d/--decode`, `-t/--text <INPUT>`, `-f/--file <INPUT>`,
`-w/--wrap <COLS>` (with `-b` as a BSD/macOS-style alias), `-i/--ignore-garbage`, and
`-o/--output <FILE>`.

Shared flags from the common CLI: `--app-header` (print the runtime header block), `--verbose`
(accepted, unused), `--log-level <level>` (case-insensitive), `--log-to-console`, `--log-to-file`,
`--rotate-log-file-by-day`.

## Exit Codes & Errors

`b64` returns `0` on success.
Invalid Base64 data when decoding exits with code `2`. I/O failures, such as unreadable input files or
write errors on the destination, exit with code `1`. A broken pipe (for example, when piping into
`head`) is treated as a successful run.