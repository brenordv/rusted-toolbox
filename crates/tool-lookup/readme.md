# Lookup

A simple, fast CLI utility to search for a text snippet inside files in a directory. 
It supports recursive or current-folder-only scanning, uses file-extension filtering, and configurable output formatting.

## Features
- Case-insensitive substring search
- Recursive search or current directory only
- Filter by file extensions (e.g., .rs, .md, .txt)
- Print matches with file path and line number, or just the matching lines
- Optional summary header with statistics

## Usage

```bash
lookup [OPTIONS] --text "" --path "  "
````

## Options
- --text <text> The text to search for (case-insensitive)
- --path <path> Base directory or file to search
- --ext <ext>... File extensions to include (repeatable). Accepts forms like txt, .txt, *.txt, Md
- --current-only Search only the current directory (no recursion)
- --line-only Prints only the matching line content (no file:line prefix)
- --no-header Does not print the final summary line

Note: If no extensions are provided, all files are searched.

## Examples

- Search recursively for “error” in all log files, starting from the current directory:
```bash
lookup --text "error" -e log
```

- Search only the current directory for “TODO” in Rust and Markdown files:
```bash
lookup --text "todo" --current-only --ext rs --ext .md
```

- Search a specific file and print only matching lines:
```bash
lookup --text "version" --path Cargo.toml --line-only 
```

- Suppress the summary header:
```bash
lookup --text "fixme" --path src --no-header
```

## Output
- Default: prints lines as `<file_path>:<line_number>| <line>`
- With `--line-only`: prints just the line content

A final summary (unless `--no-header` is set) shows the number of files scanned, total lines processed, matches found,
and the elapsed time.