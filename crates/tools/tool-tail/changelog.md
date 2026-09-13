# Changelog

## 1.2.0
- `--follow=name` no longer holds one stale descriptor per followed file: each polling cycle
  releases any held handle before reopening the path, matching the documented contract that
  name mode keeps no handle between cycles.
- Follow mode no longer retries persistent per-file stat/read failures forever. Without
  `--retry`, a source whose stat or read fails is abandoned (same policy name mode already
  applied to open failures) and the run reports it; with `--retry`, tail keeps trying as
  before. The once-per-transition warning now names the consequence on the non-retry path.
  This resolves the backlog note about the follow loop retrying persistent I/O errors
  without bound (recorded in next.md under tool-lookup, but the follow loop is tail's).

## 1.1.0
- Counts accept GNU's full suffix grammar (lowercase `k`/`m`, `B` forms as powers of 1000, `iB`
  forms as powers of 1024) and saturate at the 64-bit maximum instead of rejecting `Z` and larger
  suffixes (via shared-head-tail 1.1.0).
- When not following, regular files that stat at size 0 but yield content when read (Linux
  `/proc`) run through the streaming path, so the last-NUM forms work on them; `-f` still treats
  such a file by its reported size.

## 1.0.0
- Initial release: a port of GNU tail backed by the `shared-head-tail` engine crate.
- `-n`/`-c` with the `+NUM` from-the-start forms and the POSIX `-NUM` spelling, GNU multiplier
  suffixes, `-q`/`-v`/`-z`, multi-file headers, stdin and `-` operands.
- `--follow` (descriptor and name modes), `-F`, `--retry`, `--sleep-interval`, truncation
  detection, per-source headers while following.
- `-q` and `-v`/`--verbose` are positionally last-wins, matching GNU getopt.
- Divergences from GNU: follow mode is polling only (no inotify); rotation detection on Windows
  falls back to a length heuristic (Unix compares device/inode); `--pid` is rejected as
  unsupported and `--max-unchanged-stats` is accepted but inert; a closed output pipe ends the
  run quietly with exit 0 instead of SIGPIPE death; obsolete count syntax (`tail +5`) is not
  accepted; suffixes above `E` are rejected as too large for a 64-bit count; `--follow` takes
  its value only in the attached `--follow=name` form (GNU's optional-argument getopt behaves
  the same way); `tail -f` whose only input is stdin ends at EOF with a warning and exit 0
  instead of waiting forever.
- Exit codes: 0 success, 1 any input failed or the follow rotation emptied, 130 interrupted
  (interruption wins over an earlier failure).