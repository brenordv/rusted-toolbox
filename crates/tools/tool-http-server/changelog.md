# Changelog

## 2.1.1
- Security (Windows): the hidden-file block now checks backslash-separated path segments too.
  `PathBuf::join` honors `\` on Windows, so an URL like `/sub%5C..%5C.secret%5Cdata.txt` could
  reach a dotfile that the same path spelled with `/` correctly got a 404 for.
- The directory listing's `[DIR] ..` link goes up one level even when the request path carries
  the trailing slash browsers normalize onto directory URLs; `/sub/` previously linked back to
  itself.
- `serve_file` and `collect_directory_entries` do their file IO through `tokio::fs`, so a large
  file or a slow directory read no longer blocks a runtime worker thread. Responses still buffer
  whole files in memory; streaming and HTTP Range support remain recorded ideas, not built.

## 2.1.0
- Fixed an XSS hole in the directory listing: the page title and heading, the parent link, and every entry name and href are now HTML-escaped, and href path segments are percent-encoded so file names containing `#`, `?`, quotes, or spaces produce working, non-injectable links.
- Added graceful shutdown: the server drains in-flight connections on Ctrl+C, logs one info line, and exits with code 0.
- BREAKING: `-h` now prints help instead of being a shorthand for `--host`; `--host` is long-only.
- Validation failures are now reported on stderr before the logging subscriber boots; previously they went through `error!` before any subscriber existed and were dropped.
- Extracted `build_and_validate` from `initialize` as a testable seam and covered it (invalid host IP, port 0, nonexistent path, non-directory path), plus regression tests for path traversal rejection, index.html fallback, listing escaping in element and attribute contexts, and href percent-encoding.
- Replaced the folder/file emoji glyphs in the directory listing with plain `[DIR]`/`[FILE]` markers.
- Rewrote the readme: the command is `https` (not `http`), the web root is `-a/--path` (not positional), the flag table matches the real CLI, and the sample outputs no longer show fabricated header and log lines.

## 2.0.0
- Refactored to fit the new tooling.
- Renamed the tool to `https` to avoid conflicting with the crate `http`.

## 1.1.0
- Added `--serve-hidden` flag to optionally serve hidden files and directories (names starting with `.`).
- Hidden files are now also blocked from direct URL access by default, not just hidden from directory listings.
- Updated dependencies.

## 1.0.2
- Updated dependencies.

## 1.0.1
- Removed emojis. They don't render properly on every terminal.

## 1.0.0
Initial release