# Changelog

## 1.2.1
- The test suite uses the shared `common_cli::test_writers::FailAfter` double instead of a
  private byte-budget failing-writer copy; the behavior pinned by the tests is unchanged.

## 1.2.0
- GNU's obsolete option syntax is now accepted: `head -5` means `-n 5`, and the trailing
  letters work as in GNU (`b`/`k`/`m` switch to bytes with 512/1024/1048576 multipliers, `c`
  bytes, `l` lines, `q`/`v`/`z` the matching flags), so `head -5k` is 5120 bytes and
  `head -5kl` is 5120 lines. As in GNU, only the first argument is inspected: a `-5` behind
  `--` is still a file operand, a bare `-5` in any later position is rejected as an unknown
  option (GNU rejects it too), and later modern flags still win (`head -5 -n 3` prints 3
  lines).
- An invalid trailing letter reports `invalid trailing option -- 'X'` through the standard
  parse-error path, which exits 2 like every other parse error here (GNU exits 1 on this one
  path).

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
