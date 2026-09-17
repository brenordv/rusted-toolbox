# Changelog

## 1.1.1
- Created the readme: the three helpers and why the crate is separate from
  `common-utils`.
- The `new_guid` test now also pins the documented lowercase rendering.

## 1.1.0
- Re-exported `copy_to_clipboard`, `new_guid`, and `clean_str_regex` at the crate
  root for shorter call paths.
- Clipboard access and write failures now carry context.
- The static non-printable regex uses a documented `expect` in place of a bare
  `unwrap`, since the pattern is a compile-time constant.
- Added a crate doc, docs on `copy_to_clipboard` and `new_guid`, and tests for
  `new_guid`. `copy_to_clipboard` stays untested: it mutates the real system
  clipboard and fails headless, so it cannot run deterministically in CI.

## 1.0.0
- Initial release: clipboard, GUID, and non-printable-stripping helpers, split
  out of `shared`.
