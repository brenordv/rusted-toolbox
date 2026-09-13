# common-file-utils

File-system helpers for the toolbox. Two helpers:

- `file_system::list_all_files_recursively(&Path)`: yields every file under
  the path as a lazy iterator (backed by `walkdir`).
- `binary_sniff::is_probably_binary(&Path)`: samples the first 8 KiB and
  reports binary when the sample contains a NUL byte.

## Contracts

`list_all_files_recursively`:

- Directory entries that cannot be read (permission errors, races) and paths
  that do not exist are silently skipped, never surfaced as errors.
- A path that is a plain file yields just that file.
- Directories themselves are not yielded, only files.

`is_probably_binary`:

- A file that cannot be opened or read reports as NOT binary, so the caller's
  own open/read error handling stays in charge.
- Fail-open heuristic: NUL-free non-text content (ANSI escapes, single-byte
  encodings) reports as text. Not a security or sanitization control.
