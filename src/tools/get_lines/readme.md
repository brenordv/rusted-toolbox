# Get Lines Tool

## What it does

The Get Lines tool (`get_lines`) is a high-performance text search utility that extracts lines containing specific 
search patterns from text files.
It supports concurrent processing with multiple workers and can output results either to the console or to separate
files for each search term.

**Key Features:**
- Case-insensitive pattern matching with substring search
- Concurrent line processing with configurable worker threads
- Dual output modes: console streaming or separate files per search term
- Graceful shutdown handling with Ctrl+C support
- Optional line number display control
- Real-time progress feedback (when enabled)
- Asynchronous I/O for optimal performance

## Command-Line Options
- `-s, --search`: Required a comma-separated list of search terms (case-insensitive)
- `-f, --file`: Required a path to an input text file
- `-o, --output`: Optional output folder (creates separate .txt files per search term)
- `-w, --workers`: Optional worker thread count for parallel processing (default: 1)
- `-i, --hide-line-numbers`: Optional flag to omit line numbers from output
- `-d, --hide-runtime-info`: Optional flag to suppress startup information display

## Examples

### Basic Console Search
**Command:**
```bash
get_lines --file server.log --search "error,warning"
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
get_lines --file application.log --search "user,admin,system" --output results
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

### Multi-worker search with Multiple Workers
**Command:**
```bash
get_lines --file large_file.txt --search "critical,urgent" --workers 4 --hide-line-numbers
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

**Output (console, unordered due to parallel processing):**
```
URGENT: Security breach attempt
CRITICAL: System failure detected
Critical database error
```

### Silent Processing Mode
**Command:**
```bash
get_lines --file data.txt --search "target" --hide-runtime-info --hide-line-numbers --output silent_results
```

This mode runs without displaying startup information and processes quietly, ideal for automated scripts.

### When to Use Each:

**Use `get_lines` when:**
- You need separate output files for different search patterns
- You want built-in parallel processing
- You prefer simple substring matching
- You're working with structured log analysis workflows

**Use `grep` when:**
- You need complex regex pattern matching
- You want standard Unix tool behavior and compatibility
- You need context lines around matches
- You're working with shell scripts that expect a standard grep output format