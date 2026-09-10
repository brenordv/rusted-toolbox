# Changelog

## 1.1.0
- Counts accept GNU's full suffix grammar (lowercase `k`/`m`, `B` forms as powers of 1000, `iB`
  forms as powers of 1024) and saturate at the 64-bit maximum instead of rejecting `Z` and larger
  suffixes, so `-c 5Z` now means the whole input (via shared-head-tail 1.1.0).
- Regular files that stat at size 0 but yield content when read (Linux `/proc`) run through the
  streaming path, fixing the negative-count forms on them.

## 1.0.0
- Initial release: a port of GNU head backed by the `shared-head-tail` engine crate.
- `-n`/`-c` with the leading `-` "all but the last NUM" forms, GNU multiplier suffixes,
  `-q`/`-v`/`-z`, multi-file headers, stdin and `-` operands.
- `-q` and `-v`/`--verbose` are positionally last-wins, matching GNU getopt.
- Divergences from GNU: a closed output pipe ends the run quietly with exit 0 instead of SIGPIPE
  death; obsolete count syntax (`head -5`) is not accepted; suffixes above `E` are rejected as
  too large for a 64-bit count.
- Exit codes: 0 success, 1 any input failed, 130 interrupted (interruption wins over an earlier
  failure).
