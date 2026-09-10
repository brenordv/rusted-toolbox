# Get Lines Tool

## What it does

The Get Lines tool (`get-lines`) is a text search utility that extracts lines containing specific
search patterns from text files.
It can output results either to the console or to a separate file per search term.

**Key Features:**
- Case-insensitive substring matching for one or more search terms
- Dual-output modes: console streaming or a separate file per search term
- Output preserves the input line order
- Graceful shutdown handling with Ctrl+C support
- Optional line number display control
- Resilient to invalid (non-UTF-8) bytes: the scan continues to the end of the file

## Command-Line Options
- `-s, --search`: Required a comma-separated list of search terms (case-insensitive)
- `-f, --file`: Required a path to an input text file
- `-o, --output`: Optional output folder (creates a separate .txt file per search term)
- `-i, --hide-line-numbers`: Optional flag to omit line numbers from output
- `--app-header`: Optional flag to print the tool header and runtime configuration

## Behavior notes

- **Exit codes:** `0` on completion, `1` on failure (for example, the input cannot be opened or an
  output file cannot be written), and `130` when the run is interrupted with Ctrl+C. An interrupted
  run flushes and keeps whatever partial output was already written.
- **Ordered output:** matched lines are written in the order they appear in the input.
- **Multiple terms per line:** in console mode a line is printed once even if it matches several
  terms. In file mode the line is written to each matched term's file.
- **Invalid UTF-8:** invalid bytes are replaced with the Unicode replacement character (`�`) and the
  rest of the file is still searched, so a stray byte never truncates the results.
- **Line length:** a single line larger than 64 MiB with no newline aborts the run with an error,
  rather than buffering the whole input into memory.
- **Search terms:** terms are trimmed, lowercased, and deduplicated. Terms containing only
  filesystem-unsafe characters, or that collide after sanitizing, are given a distinct output
  filename, so no term overwrites another's file. When every term is pure ASCII, matching uses a
  case-insensitive ASCII fast path, so an input character whose Unicode lowercase folds into ASCII
  (such as the Turkish dotted 'İ') does not match in that mode. A single non-ASCII term switches
  all terms to Unicode-aware matching.

## Examples

### Basic Console Search
**Command:**
```bash
get-lines --file server.log --search "error,warning"
```

**Input (server.log):**
```
2024-01-15 10:30:12 INFO Server started successfully
2024-01-15 10:31:45 ERROR Database connection failed
2024-01-15 10:32:01 WARNING Low disk space detected
2024-01-15 10:33:15 INFO User login successful
2024-01-15 10:34:22 ERROR Authentication timeout
```

**Output (console):**
```
2	2024-01-15 10:31:45 ERROR Database connection failed
3	2024-01-15 10:32:01 WARNING Low disk space detected
5	2024-01-15 10:34:22 ERROR Authentication timeout
```

### File-Based Output with Multiple Search Terms
**Command:**
```bash
get-lines --file application.log --search "user,admin,system" --output results
```

**Input (application.log):**
```
User john logged in
Admin panel accessed
System maintenance started
User alice updated profile
Admin rights granted
System backup completed
```

**Output:**
- `results/user.txt`:
```
1	User john logged in
4	User alice updated profile
```
- `results/admin.txt`:
```
2	Admin panel accessed
5	Admin rights granted
```
- `results/system.txt`:
```
3	System maintenance started
6	System backup completed
```

### Hiding Line Numbers
**Command:**
```bash
get-lines --file large_file.txt --search "critical,urgent" --hide-line-numbers
```

**Input (large_file.txt):**
```
Normal operation message
CRITICAL: System failure detected
Routine maintenance log
URGENT: Security breach attempt
Standard information log
Critical database error
```

**Output (console):**
```
CRITICAL: System failure detected
URGENT: Security breach attempt
Critical database error
```

### When to Use Each:

**Use `get-lines` when:**
- You need separate output files for different search patterns
- You prefer simple substring matching
- You're working with structured log analysis workflows

**Use `grep` when:**
- You need complex regex pattern matching
- You want standard Unix tool behavior and compatibility
- You need context lines around matches
- You're working with shell scripts that expect a standard grep output format