# Lookup

A simple, fast CLI utility to either:
- search for a text snippet inside files (subcommand `text`), or
- find files by name using wildcard or regex patterns (subcommand `files`).

Both modes default to case-insensitive matching. The tool supports recursive or current-folder-only scanning,
configurable output, and per-subcommand headers.

## Features
- Two subcommands: `text` (content search) and `files` (filename search)
- Case-insensitive by default (`files` can be made case-sensitive)
- Recursive search (default) or current directory only
- Extension filtering for text search
- Binary files are skipped during text search
- Clean progress output for `files` search (single-line updates)
- Per-subcommand summary controls

## Installation
This crate is part of Rusted Toolbox. Build with:
```bash
cargo build -p lookup
```

## Usage
General form:
```bash
lookup <SUBCOMMAND> [OPTIONS]
```
If no subcommand is provided, the app prints usage and exits.

### Subcommands

#### 1) `text`: search for text inside files
```bash
lookup text <TEXT> [OPTIONS]
```
Arguments and options:
- `<TEXT>`                       Text to search for (required positional)
- `-p, --path <PATH>`            Where to search (default: current directory)
- `-e, --extension <EXT>` ...    File extensions to include (repeatable). Accepts forms like `txt`, `.txt`, `*.txt`, `Md`. If omitted, all files are searched.
- `-c, --current-only`           Search only the current directory (no recursion)
- `-l, --line-only`              Print only the matching line content (no file:line prefix)
- `-m, --no-summary`             Do not print the final summary line

Behavior:
- Files whose first 8 KiB contain a NUL byte are treated as binary and skipped. Each skip is counted and logged at debug level.
- Lines that are not valid UTF-8 are skipped and counted.

Output:
- Default: `<file_path>:<line_number>| <line>`
- With `--line-only`: just the line content
- Summary (unless `--no-summary`): files searched, lines scanned, matches found, binary files skipped, invalid UTF-8 lines skipped, and elapsed time

Examples:
```bash
# Recursively search for "error" in *.log files from the current directory
lookup text "error" -e log

# Search only the current directory for "TODO" in Rust and Markdown files
lookup text "todo" --current-only -e rs -e .md

# Search a specific file and print only matching lines
lookup text "version" --path Cargo.toml --line-only

# Suppress the summary line
lookup text "fixme" --path src --no-summary
```

#### 2) `files`: find files by filename
Use wildcard (default) or regex. Case-insensitive by default.
```bash
lookup files <PATTERN>... [OPTIONS]
```
Arguments and options:
- `<PATTERN>...`                  Filename pattern(s) to match (at least one required)
- `-p, --path <PATH>`             Where to search (default: current directory)
- `-s, --file-search-pattern <wildcard|regex>`  Pattern mode used to match filenames (default: wildcard)
- `-c, --case-sensitive`          Make pattern matching case-sensitive (default: insensitive)
- `-n, --no-recursive`            Search only the current directory (no recursion)
- `-o, --no-progress`             Suppress progress updates
- `-e, --no-errors`               Suppress error messages during traversal
- `-m, --no-summary`              Suppress the final summary output

Behavior:
- Prints the absolute path to each match (with Windows verbatim prefixes like `\\?\` removed for readability)
- Shows progress as: `Reading: <folder>` updated on the same line; lines are cleared to avoid overlap
- On traversal errors: clears the progress line, prints a brief error message, and continues
- At the end, prints a summary with total dirs, files, matches, and elapsed time (unless `--no-summary`)

Examples:
```bash
# Wildcard search (default), recursive
lookup files "*.rs"

# Multiple wildcard patterns from a parent folder, without progress noise
lookup files "README.*" "LICENSE*" -p .. --no-progress

# Regex: match exactly mydoc.pdf, mydoc.epub, or mydoc.mobi
lookup files -s regex "^mydoc\.(pdf|epub|mobi)$"

# Case-sensitive regex
lookup files -s regex --case-sensitive "^[A-Z].*\.MD$"

# Current folder only; suppress errors and summary
lookup files "*.log" --no-recursive --no-errors --no-summary
```

## Notes
- Pass the shared `--app-header` flag to print the tool header and the subcommand's runtime config before the search runs.
- Progress rendering uses ANSI control sequences to clear the line; on non-ANSI terminals you can pass `--no-progress`.
