# 1.0.0
- Initial release.
- Extracts regex-matched values from text files, one per line: group 1 when the pattern has
  capture groups, the whole match otherwise, every non-overlapping match per line.
- Three modes: `all`, `unique-per-file`, `unique-per-run` (first occurrence wins).
- Self-contained wildcard expansion (`*`, `?`, `[ab]`, `{a,b}`, `**`) on every platform;
  case-insensitive on Windows, case-sensitive elsewhere.
- `-H/--with-filename` prefixes each value with its source file.
- Bytes-level matching (invalid UTF-8 passes through), one sequential pass per file, 8 MiB
  line cap with per-file containment.
- Exit codes: 0 completed (even with no matches), 1 any target or file failure, 130
  interrupted.
