# Lookup

A simple, fast CLI utility to either:
- search for a text snippet inside files (subcommand `text`), or
- find files by name using wildcard or regex patterns (subcommand `files`).

Both modes default to case-insensitive matching. The tool supports recursive or current-folder-only scanning,
configurable output, and per-subcommand headers.

## Features
- Two subcommands: `text` (content search) and `files` (filename search)
- Case-insensitive by default (can be made case-sensitive where applicable)
- Recursive search (default) or current directory only
- Extension filtering for text search
- Clean progress output for `files` search (single-line updates)
- Per-subcommand summary/header controls

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

#### 1) `text` — search for text inside files
Preferred positional syntax keeps it concise:
```bash
lookup text "your text" [OPTIONS]
```
Legacy flag is also supported:
```bash
lookup text --text "your text" [OPTIONS]
```
Options:
- `-t, --text <TEXT>`            Text to search for (alternative to positional)
- `-p, --path <PATH>`            Where to search (default: current directory)
- `-e, --extension <EXT>` ...    File extensions to include (repeatable). Accepts forms like `txt`, `.txt`, `*.txt`, `Md`.
- `-c, --current-only`           Search only the current directory (no recursion)
- `-l, --line-only`              Print only the matching line content (no file:line prefix)
- `-n, --no-header`              Do not print the final summary line

Notes:
- At least one `--extension/-e` must be provided.

Output:
- Default: `<file_path>:<line_number>| <line>`
- With `--line-only`: just the line content
- Summary (unless `--no-header`): number of files scanned, total lines processed, matches found, and elapsed time

Examples:
```bash
# Recursively search for "error" in *.log files from the current directory
lookup text "error" -e log

# Search only the current directory for "TODO" in Rust and Markdown files
lookup text "todo" --current-only -e rs -e .md

# Search a specific file and print only matching lines
lookup text --text "version" --path Cargo.toml --line-only

# Suppress the summary header
lookup text "fixme" --path src --no-header
```

#### 2) `files` — find files by filename
Use wildcard (default) or regex. Case-insensitive by default.
```bash
lookup files <PATTERN>... [OPTIONS]
```
Options:
- `-p, --path <PATH>`            Where to search (default: current directory)
- `--regex`                      Use regex patterns (default mode is wildcard)
- `--wildcard`                   Explicitly choose wildcard/glob mode (default)
- `--case-sensitive`             Make pattern matching case-sensitive (default: insensitive)
- `-c, --current-only`           Search only the current directory (no recursion)
- `-n, --no-header`              Suppress header output
- `--no-progress`                Suppress progress updates (current folder)
- `--no-errors`                  Suppress error messages during traversal
- `--no-summary`                 Suppress the final summary output

Behavior:
- Prints the absolute path to each match (with Windows verbatim prefixes like `\\?\` removed for readability)
- Shows progress as: `Reading: <folder>` updated on the same line; lines are cleared to avoid overlap
- On traversal errors: clears the progress line, then prints a brief error message as, and continues
- At the end, prints a summary with total dirs, files, matches, and elapsed time (unless `--no-summary`)

Examples:
```bash
# Wildcard search (default), recursive
lookup files "*.rs"

# Multiple wildcard patterns from a parent folder, without progress noise
lookup files "README.*" "LICENSE*" -p .. --no-progress

# Regex: match exactly mydoc.pdf, mydoc.epub, or mydoc.mobi
lookup files --regex "^mydoc\.(pdf|epub|mobi)$"

# Case-sensitive regex
lookup files --regex --case-sensitive "^[A-Z].*\.MD$"

# Current folder only; suppress errors and summary
lookup files "*.env" --current-only --no-errors --no-summary
```

## Notes
- Each subcommand prints its own header unless `--no-header` is passed.
- Progress rendering uses ANSI control sequences to clear the line; on non-ANSI terminals you can pass `--no-progress`.