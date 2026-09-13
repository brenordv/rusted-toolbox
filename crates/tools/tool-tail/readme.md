# tail
## What It Is
A port of the GNU coreutils `tail` command to Rust, useful on Windows where it does not exist
as a native command. Prints the last part of files (the last 10 lines of each FILE by default)
and can follow files as they grow. Its sibling [head](../tool-head/readme.md) covers the other
end of the file; both are built on the shared `shared-head-tail` engine crate.

## Examples
### Last lines of a file
```bash
tail -n 5 app.log
```

### Last bytes
```bash
tail -c 100 app.log
```

### From line 20 to the end
```bash
tail -n +20 app.log
```

### Follow a log file
```bash
tail -f app.log
tail -F app.log          # follow by name and keep retrying (log rotation)
tail -f -s 0.5 app.log   # poll twice a second
```

## Command-line options
### Tool flags
| Flag                        | Description                                                                                                                          |
|-----------------------------|--------------------------------------------------------------------------------------------------------------------------------------|
| `-n`, `--lines [+]NUM`      | Print the last NUM lines (default 10); `+NUM` starts at line NUM; `-NUM` is the explicit POSIX spelling of the default               |
| `-c`, `--bytes [+]NUM`      | Print the last NUM bytes; `+NUM` starts at byte NUM                                                                                  |
| `-f`, `--follow[=MODE]`     | Output appended data as the file grows; MODE is `descriptor` (default) or `name` and binds only in the attached form `--follow=name` |
| `-F`                        | Same as `--follow=name --retry`                                                                                                      |
| `--retry`                   | Keep trying files that cannot be opened, statted, or read; without it, a file that keeps failing is abandoned                        |
| `-s`, `--sleep-interval N`  | Seconds between follow checks (default 1.0; floating point accepted)                                                                 |
| `--max-unchanged-stats N`   | Accepted for GNU compatibility; has no effect in this port                                                                           |
| `--pid PID`                 | Not supported by this port; rejected with an error                                                                                   |
| `-q`, `--quiet`, `--silent` | Never print headers giving file names                                                                                                |
| `-v`, `--verbose`           | Always print headers giving file names                                                                                               |
| `-z`, `--zero-terminated`   | Line delimiter is NUL, not newline                                                                                                   |

Counts accept the GNU multiplier suffixes: `b` (512) and the scale letters `K`, `M`, `G`, `T`,
`P`, `E`, `Z`, `Y`, `R`, `Q`. A bare letter or its `iB` form means powers of 1024 (`K` = `KiB` =
1024); its `B` form means powers of 1000 (`kB` = `KB` = 1000). `k` and `m` also work in
lowercase, as in GNU. A count too large for 64 bits saturates at the maximum instead of failing,
so `-c 5Z` means the whole input.

Repeated `-n`/`-c` flags, or both together, are last-wins on the command line, matching GNU
getopt; so are `-q` and `-v`/`--verbose` against each other.

### Shared flags
These flags are common to every tool in this workspace and have no short form:

| Flag                       | Description                                                                                                                   |
|----------------------------|-------------------------------------------------------------------------------------------------------------------------------|
| `--app-header`             | Print the header with tool name, version, and runtime options before the output                                               |
| `--log-level <LEVEL>`      | Set the log level: `trace`, `debug`, `info`, `warn`, `warning`, `error`, or `disabled` (case-insensitive). Defaults to `warn` |
| `--log-to-console`         | Send log output to stdout instead of the default stderr                                                                       |
| `--log-to-file`            | Also write log output to a file                                                                                               |
| `--rotate-log-file-by-day` | Rotate the log file daily                                                                                                     |

## Exit codes
| Code | Meaning                                                                     |
|------|-----------------------------------------------------------------------------|
| 0    | Every input was processed (also: the output pipe closed early, see below)   |
| 1    | At least one input failed, or the follow rotation ran out of files          |
| 130  | Interrupted with Ctrl+C; interruption wins over an earlier per-file failure |

## Notes and caveats
- With no FILE, or when FILE is `-`, tail reads standard input. Run in a terminal with no path,
  it waits for typed input; end it with Ctrl+D (Ctrl+Z then Enter on Windows). This is the pipe
  idiom, not a hang.
- A missing or unreadable file is reported on stderr and processing continues with the remaining
  files; the exit code is 1 at the end.
- When the reading end of a pipe closes early, the run stops quietly and exits 0. GNU dies from
  SIGPIPE (exit 141) instead; re-raising the signal is not portable to Windows.
- File content and file names pass through unmodified. Hostile content or names can carry
  terminal control sequences, and a file name containing a newline can visually forge a header
  line.
- When stdout is a real Windows console, the OS can reject byte output that is not valid UTF-8.
  Redirected or piped output is unaffected and byte-exact.
- The last-NUM forms hold up to NUM items in memory on non-seekable input (pipes, stdin); items
  are lines, so pathological inputs with huge lines grow accordingly, the same way GNU's pipe
  paths do. Seekable files run in constant memory.
- A regular file that reports size 0 but still yields content when read (Linux `/proc`-style
  virtual files) is handled through the streaming path, so the last-NUM forms work on it. Follow
  mode still trusts the reported size and does not handle such files.
- Obsolete option syntax (`tail +5`) is not accepted; use `-n +5`.
- On non-Windows systems the build scripts do not copy this binary into `dist/`, since coreutils
  already provides `tail`.

### Follow mode
- Follow is polling only; there is no inotify/kqueue watching. Changes appear within
  `--sleep-interval` seconds, and the loop wakes each interval even when nothing changed.
- Only regular-file operands are followed. Standard input and non-regular files are read once;
  if nothing followable remains, tail says so and exits 0 instead of waiting forever on stdin
  (GNU waits indefinitely there).
- `tail -f missing.txt` without `--retry` reports the file, then `no files remaining`, and
  exits 1.
- Truncation is detected by the file shrinking below the last read position; tail reports it and
  starts over from the beginning.
- Rotation detection in `--follow=name` compares device/inode identity on Unix. Windows has no
  such identity check here, so a same-name replacement file of equal or greater length is not
  detected and output continues from a stale offset; a shorter replacement is caught by the
  truncation check.