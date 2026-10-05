# common-file-utils

File-system helpers for the toolbox:

- `file_system::list_all_files_recursively(&Path)`: yields every file under
  the path as a lazy iterator (backed by `walkdir`).
- `binary_sniff::is_probably_binary(&Path)`: samples the first 8 KiB and
  reports binary when the sample contains a NUL byte.
- `permissions::create_dir_all_owner_only(&Path)` and
  `permissions::open_owner_only(&mut OpenOptions, &Path)`: directory creation
  and file opening restricted to the owner (0o700 / 0o600) on Unix, default
  ACL inheritance on Windows.
- `atomic_write::write_via_temp(&Path, FnOnce(&PathBuf) -> Result<()>)`: runs
  the closure against a temp file in the destination's directory, then renames
  it over the destination.

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

`write_via_temp`:

- A failed closure leaves the destination untouched and removes the temp file;
  the closure's error is returned unchanged.
- The temp file is created next to the destination so the final rename never
  crosses filesystems.
- A replaced destination keeps its own permissions; a fresh one gets what a
  plain `File::create` would produce (Unix `0o666 & !umask`, Windows the
  directory's ACL).
