# shared-head-tail

Shared engine machinery behind the `head` and `tail` tool crates:

- `count_parser`: the GNU count grammar for `-n`/`-c` values, with sign
  prefixes (`-NUM`, `+NUM`) and multiplier suffixes: `b`, then the scale
  letters `kKmMGTPEZYRQ` with optional `B` (powers of 1000) or `iB` (powers
  of 1024) modifiers. A value too large for 64 bits saturates at `u64::MAX`.
- `io_shared`: the byte-oriented plumbing both engines run on: the backward
  scan that finds where the last NUM items start, chunked copy helpers, the
  per-input run driver with header policy and continue-on-error accounting,
  the `Interrupted` Ctrl+C marker, the file name header state, and the exit
  mapping for the shared 0/1/130 contract. Re-exports `common-cli`'s
  broken-pipe helpers so a closed output pipe ends a run quietly.
- `models`: the count, header policy, and run outcome types shared by both
  tools.

The tool-specific pieces (argument surfaces, the head and tail engines,
follow mode) live in `crates/tools/tool-head` and `crates/tools/tool-tail`.
