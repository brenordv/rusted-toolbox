# common-file-utils

Recursive file-system listing for the toolbox. One helper:

- `list_all_files_recursively(&Path)`: yields every file under the path as a
  lazy iterator (backed by `walkdir`).

## Contract

- Directory entries that cannot be read (permission errors, races) and paths
  that do not exist are silently skipped, never surfaced as errors.
- A path that is a plain file yields just that file.
- Directories themselves are not yielded, only files.
