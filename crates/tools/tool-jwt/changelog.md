# 2.0.4
- The claim printers no longer panic when the consumer closes the output pipe
  (`jwt ... | head`): they write through the shared broken-pipe helpers and main treats a
  closed pipe as a normal end of output, exiting 0. The clipboard copy (`-c`), which does
  not depend on stdout, still runs after a closed pipe.
- The printers take an output writer and return errors instead of printing directly, so
  write failures other than a closed pipe are reported through the normal error path.

# 2.0.3
- A JSON-formatting failure now exits through the shared flush-aware error path instead of a raw
  `process::exit(1)` that could drop buffered output.
- "Claim not found" moved from a raw stderr print to a warning log, so it is visible at the
  default level and now respects `--log-level` and `--log-to-file`; the exit code stays 0.
- The empty-claims message dropped from error to warning; it exits 0 as documented, and error
  level implied a failure that never was.
- App errors are reported once by main: the decode-failure message no longer doubles its
  "Error decoding token:" prefix.
- `jwt_app.rs` no longer calls exit helpers; fallible operations return errors and main decides
  the exit.

# 2.0.2
- Fixed a startup regression from 2.0.0: running without an explicit `--print` failed with
  "invalid value 'Pretty'" because the declared default rendered its Display form, which the
  value parser rejects. The default is now the parser-facing `pretty`, and a clap
  `debug_assert` test plus a default-parse regression test pin it.
- An `exp` claim that is present but not an integer no longer reports "No expiration claim". A fractional NumericDate (allowed by RFC 7519) is truncated toward zero and evaluated normally; a non-numeric value (string, boolean, null, array, object) reports "Invalid expiration". The invalid-status text is now "Invalid expiration (exp out of range or not numeric)".
- Readme: the JSON example now shows claims in token order (`sub`, `name`, `iat`), matching the pretty example; workspace builds preserve the token's claim order through serde_json's `preserve_order` feature.

# 2.0.1
- An `exp` claim outside the timestamp range chrono can represent is now reported as "Invalid expiration (exp out of range)" instead of being silently treated as expired just now.
- Extracted pure helpers for CSV formatting (`format_csv`) and clipboard claim lookup (`resolve_claim_value`), and added tests covering them, token normalization, and expiration edge cases.
- The tool header section now renders through the shared `common-cli` header formatters; output bytes are unchanged.
- Corrected the readme: the header only prints with `--app-header`, shows `jwt (version)` with the runtime-config block, and contains no emojis.

# 2.0.0
- Refactored to fit the new tooling.

# 1.0.3
- Updated dependencies.

# 1.0.2
- Removed emojis. They don't render properly on every terminal.

# 1.0.1
- The name of the property to copy is now case-insensitive.
- Fixed copying string properties to the clipboard. Now we don't copy the quotes.

# 1.0.0
Initial release