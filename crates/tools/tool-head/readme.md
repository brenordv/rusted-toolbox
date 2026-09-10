# head
## What It Is
A port of the GNU coreutils `head` command to Rust, useful on Windows where it does not exist
as a native command. Prints the first part of files: the first 10 lines of each FILE by
default. Its sibling [tail](../tool-tail/readme.md) covers the other end of the file; both are
built on the shared `shared-head-tail` engine crate.

## Examples
### First lines of a file
```bash
head -n 5 app.log
```

### All but the last 2 lines
```bash
head -n -2 app.log
```

### First bytes
```bash
head -c 100 app.log
```

### Multiple files
```bash
$ head -n 1 a.txt b.txt
```
Output:
```
==> a.txt <==
first line of a

==> b.txt <==
first line of b
```

## Command-line options
### Tool flags
| Flag                        | Description                                                                                  |
|-----------------------------|----------------------------------------------------------------------------------------------|
| `-n`, `--lines [-]NUM`      | Print the first NUM lines (default 10); with a leading `-`, print all but the last NUM lines |
| `-c`, `--bytes [-]NUM`      | Print the first NUM bytes; with a leading `-`, print all but the last NUM bytes              |
| `-q`, `--quiet`, `--silent` | Never print headers giving file names                                                        |
| `-v`, `--verbose`           | Always print headers giving file names                                                       |
| `-z`, `--zero-terminated`   | Line delimiter is NUL, not newline                                                           |

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
| 1    | At least one input failed                                                   |
| 130  | Interrupted with Ctrl+C; interruption wins over an earlier per-file failure |

## Notes and caveats
- With no FILE, or when FILE is `-`, head reads standard input. Run in a terminal with no path,
  it waits for typed input; end it with Ctrl+D (Ctrl+Z then Enter on Windows). This is the pipe
  idiom, not a hang.
- A missing or unreadable file is reported on stderr and processing continues with the remaining
  files; the exit code is 1 at the end.
- When the reading end of a pipe closes early (`head big.txt | grep -m1 x`), the run stops
  quietly and exits 0. GNU dies from SIGPIPE (exit 141) instead; re-raising the signal is not
  portable to Windows.
- File content and file names pass through unmodified. Hostile content or names can carry
  terminal control sequences, and a file name containing a newline can visually forge a header
  line.
- When stdout is a real Windows console, the OS can reject byte output that is not valid UTF-8.
  Redirected or piped output is unaffected and byte-exact.
- The negative-count forms hold up to NUM items in memory on non-seekable input (pipes, stdin);
  items are lines, so pathological inputs with huge lines grow accordingly, the same way GNU's
  pipe paths do. Seekable files run in constant memory.
- A regular file that reports size 0 but still yields content when read (Linux `/proc`-style
  virtual files) is handled through the streaming path, so the negative-count forms work on it.
- Obsolete option syntax (`head -5`) is not accepted; use `-n 5`.
- On non-Windows systems the build scripts do not copy this binary into `dist/`, since coreutils
  already provides `head`.