# Changelog

## 1.1.0
- The count grammar now matches GNU's parser rather than only its documentation: lowercase `k`
  and `m` are accepted, and the `B`/`iB` modifiers combine with every scale letter (`KB`, `kiB`,
  `mB`, ...), the `bkKmMGTPEZYRQ0` set coreutils feeds xstrtol.
- A count too large for 64 bits (overflowing digits, or `Z`/`Y`/`R`/`Q` suffixes scaling past
  the range) saturates at `u64::MAX` instead of being rejected, matching GNU's quiet clamp;
  `0Z` still scales to 0.
- `process_input_source` routes a file whose reported length is 0 through the streaming path, so
  procfs-style virtual files (stat size 0, real content) behave like pipes instead of getting
  wrong output from the seek-based fast path; genuinely empty files produce the same output
  either way.
- Readme: the `count_parser` summary now states this grammar (scale letters to `Q`, `B`/`iB`
  modifiers, saturation) instead of the 1.0.0 reject-as-overflow wording.

## 1.0.0
- Initial release: the shared engine crate behind the head and tail tool crates (count grammar,
  backward scan, chunked copy helpers, per-input run driver, interrupt marker, header state, and
  the 0/1/130 exit mapping).