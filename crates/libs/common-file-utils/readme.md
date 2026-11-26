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
- `adoption::adopt(Option<&Path>, &Path, Shape) -> Adoption`: moves a tool's
  data from its old default place to its new one, once.

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

`adopt`:

- Moves only when the old place holds data and the new path is absent: a file by
  a hard link at the new path and then removing the old name, a folder by one
  rename. The new path's folders are created owner-only (0o700 on Unix).
- Never copies and never overwrites: across filesystems, or on a filesystem
  with no hard links, the move fails and the data stays in the old place
  (`KeptLegacy`).
- Never moves or removes a symbolic link. When both places hold data, the same
  file behind both (`same_file::is_same_file`) is one file under two names, and
  its old name is removed only when the two paths are different directory
  entries once links and junctions are resolved (`fs::canonicalize`): one entry
  reached twice holds the only copy. Two different files, or a failed check,
  are `BothPresent`, and the new path is used.
- A failed step is followed by a fresh look at both paths, never a decision on
  the error, so a rival that moved the data first gives `AlreadyThere`.
- A file whose old name cannot be removed (on Windows, another program has it
  open) is used at the new path and reported as `OldNameLeft`; a later save
  replaces only the new name. A database (`Shape::Database`) whose old name
  cannot be removed stays in use under the old name, since a database must
  never be opened under two names at once; so does a folder found under two
  names whose old one cannot be removed. When the old name is gone by
  the time it is looked at again, a rival removed it: `AlreadyThere`.
- Logs nothing; `Adoption::report` logs the outcome once logging is up.
- `adopt_in_use` takes the same decisions for data that looks in use, moving
  and removing nothing: what `adopt` would move stays in the old place
  (`KeptReason::InUse`).
