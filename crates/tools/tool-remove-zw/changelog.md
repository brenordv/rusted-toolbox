# Changelog

## 1.1.0 - 2026-08-03
- Strip a leading UTF-8 byte-order mark (BOM) from text files, counted separately from zero-width characters.
- Add `--dry-run` / `-d`: report which files would be modified and how, without writing anything.
- Fix `--output FILE`: the file is now written even when the input is already clean (previously nothing was created).
- Fix `--in-place`: it now works on directory inputs, and for stdin it is a no-op that writes to stdout, as its help already claimed.
- Skip UTF-16 / UTF-32 files with a clear reason instead of a generic "binary" skip (transcoding them is out of scope).
- Harden in-place writes: rename over the original without deleting it first, and carry the original file's permissions onto the replacement.
- Print the runtime header to stderr so stdout holds only the cleaned content or the dry-run report.
- Spell out in the header and `--help` that with no path it reads standard input and works as a filter, so the wait for input is expected rather than confusing.

## 1.0.0 - 2026-01-29
- Initial release of the `remove-zw` tool.
- Removes all Unicode format (Cf) characters from stdin, files, or directories.
- Non-destructive by default (writes `<stem>.cleaned<ext>`); supports `--in-place`, `--output`, `--recursive`, and `--extensions`.
