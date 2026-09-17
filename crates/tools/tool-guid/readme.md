# guid

Generates UUIDv4 values: a single guid, the all-zeros empty guid, or N guids in
one run. A single guid can be copied straight to the clipboard.

## Usage

```bash
# One guid to stdout
guid

# The all-zeros guid
guid --empty

# One guid, also copied to the clipboard
guid --copy-to-clipboard

# Five guids, one per line
guid --multiple 5
```

## Command-line options

| Flag                      | Description                                                                                  |
|---------------------------|----------------------------------------------------------------------------------------------|
| `-m, --multiple <N>`      | Generate N guids, one per line. Conflicts with `-c` and `-e`. N must be at least 1.          |
| `-c, --copy-to-clipboard` | Copy the generated guid to the system clipboard. Single-guid mode only.                      |
| `-e, --empty`             | Generate the all-zeros guid (`00000000-0000-0000-0000-000000000000`). Single-guid mode only. |

Shared flags from the common CLI: `--app-header` (print the tool header block),
`--log-level <level>` (long form only), `--log-to-console`, `--log-to-file`,
`--rotate-log-file-by-day`.

## Behavior notes

- Output goes to stdout, one guid per line; diagnostics go to stderr, so the
  output can be piped or captured cleanly.
- A consumer that closes the pipe early (`guid -m 1000 | head -5`) ends the run
  quietly with exit code 0; only the requested lines are produced.
- Clipboard copy failures are reported and exit with code 1.
- Exit codes: 0 on success (including a consumer-closed pipe), 1 on failure
  (invalid count, write error, clipboard error).