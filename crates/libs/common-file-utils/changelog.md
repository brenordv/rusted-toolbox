# Changelog

## 2.0.1
- Created the readme: the helper, the silent-skip contract, file-path input
  behavior.
- Added a test pinning that a nonexistent path yields no files (the silent-skip
  contract).

## 2.0.0
- Removed the unused `monitor_folder` async watcher module: its debounce drained
  accumulated paths under the latest event's kind, the sample handler did not
  satisfy the module's own handler bound, its two entry points disagreed on
  missing-folder behavior, and its watch loop could not terminate. It had zero
  consumers and remained in git history if a watcher is ever needed again.
- Pruned the dependencies the watcher required (`notify`, `tokio`, `serde`,
  `tracing`, `common-utils`, `anyhow`); only `walkdir` remains.
- `list_all_files_recursively` now takes `&Path` instead of `&PathBuf`.
- Documented that unreadable directories are skipped and that a file path yields
  just that file.
- Added the first unit tests.

## 1.0.0
- Initial release: recursive file listing and a folder watcher, split out of
  `shared`.