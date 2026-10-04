# Changelog

## 2.3.0
- New `atomic_write` module: `write_via_temp`, promoted from tool-image's private
  `encode_via_temp` so `seal` can share it. Stages the output in a temp file next to the
  destination, renames it into place on success, and removes it on failure, so the
  destination is never half-written. A replaced destination keeps its own permissions;
  on Unix a fresh output gets the usual `0o666 & !umask` instead of tempfile's 0o600.
  Its tests moved over with it; tool-image now calls the shared helper.

## 2.2.0
- New `permissions` module: `create_dir_all_owner_only` and `open_owner_only`, extracted from
  `common-cli`'s log-file helpers so `shared-crypto` can reuse them for identity files. On Unix,
  created directories are `0o700`, and files open with mode `0o600`, with a pre-existing file
  tightened to 0o600 through the open handle (the open fails if the tighten fails); on Windows
  both fall back to default ACL inheritance. The unix permission tests moved over with them.

## 2.1.0
- New `binary_sniff` module with `is_probably_binary`, promoted verbatim from tool-lookup's
  3.1.0 sniff so other tools can use it: samples the first 8 KiB and reports binary when the
  sample contains a NUL byte. Unreadable or unopenable files report as text, leaving the
  caller's own error handling in charge; the docs flag the helper as a fail-open heuristic,
  not a security control. Its tests moved over with it, plus a new one pinning the
  nonexistent-path contract.

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