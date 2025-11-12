# b64

`b64` is a flexible Base64 encoder/decoder for the terminal. It embraces familiar GNU-style behavior from [base64](https://www.gnu.org/software/coreutils/manual/html_node/base64-invocation.html), 
while adding helpful quality-of-life improvements:

- Encode or decode from files, stdin, or inline text.
- Automatic detection: `b64 input` treats `input` as a file when it exists, otherwise as literal text.
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

### Decode Inline Text with Garbage Filtering
```bash
$ b64 -d --text " aGVs\nbG8gd29ybGQ=\t" --ignore-garbage
hello world
```

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

## Exit Codes & Errors

`b64` returns `0` on success. 
Typical failure modes include unreadable input files, invalid Base64 data when decoding, or write errors on the
destination. A broken pipe (for example, when piping into `head`) is treated as a successful run.