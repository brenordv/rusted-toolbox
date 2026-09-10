# Changelog

## 2.0.1
- Readme: added a Flags section listing the short forms (`-d`, `-t`, `-f`, `-w` with the `-b`
  alias, `-i`, `-o`) and the shared common-cli flags.
- Added unit tests for `WrapWriter` (newline at each wrap boundary, trailing newline deferred to
  `finish()`, no-op finish on boundary-aligned or empty output, passthrough without wrap), for
  directory-path input inference, for empty-input encode with wrap enabled, and for
  `--ignore-garbage` decode of whitespace-only and padding-only input.
- Fixed the readme `--ignore-garbage` example: the old inline-text form used `\n`/`\t` inside double
  quotes, which a shell passes through literally, so the letters `n` and `t` corrupted the decode.
  The example now pipes real whitespace through `printf`.
- Documented in the readme the input inference edge cases (an existing directory is treated as
  literal text; a permission-denied path is treated as a file) and the exit codes (decode data
  errors exit `2`, I/O errors exit `1`).
- Normalized changelog heading levels so every version uses the same level.

## 2.0.0
- Refactored to fit the new tooling

## 1.0.0
- Initial release of the `b64` tool.
- Supports Base64 encode/decode from stdin, files, or inline text.
- Adds auto-detect input mode plus `--file`/`--text` overrides.
- Provides configurable wrapping and tolerant decode with `--ignore-garbage`.
