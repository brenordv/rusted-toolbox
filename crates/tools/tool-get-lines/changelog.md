# Changelog

## 2.0.1
- Runtime info lines in the `--app-header` block are now rendered through the shared
  `common-cli` header formatting. The printed bytes are unchanged.
- Added a matcher test pinning that a single non-ASCII term routes all matching through the
  Unicode engine while the ASCII terms in the same set still match.
- Documented the ASCII fast-path caveat (an input character whose Unicode lowercase folds into
  ASCII does not match when every term is ASCII) in the readme's search-terms note.

## 2.0.0
- Refactored to fit the new tooling.
- Replaced the async worker pipeline with a single-pass sequential engine. Output is now 
  written in input order.
- Console output now prints each matching line once, even when it matches several search terms.
  In file mode a matching line is still written to every matched term's file.
- Removed the `-w/--workers` flag. It never parallelized the search, so the "parallel
  processing" behavior it implied did not exist. Invocations that pass it will now error.
- Removed the dead `-d/--hide-runtime-info` flag (use `--app-header` to show the header).
- Exit codes now carry meaning: `0` on completion, `1` on failure, and `130` when the run is
  interrupted with Ctrl+C. Previously every case exited `0`.
- Invalid (non-UTF-8) bytes no longer silently truncate the scan. Such bytes are replaced with
  the Unicode replacement character and the rest of the file is still searched.
- Write and flush failures (for example, a full disk) are now reported and produce a non-zero
  exit instead of a silently truncated output file.
- A single line larger than 64 MiB with no newline now aborts with a clear error instead of
  buffering the input into memory.
- Duplicate search terms are removed, and per-term output filenames that would collide or hit a
  Windows reserved device name are disambiguated so no term overwrites another's output.
- ASCII search terms use a case-insensitive Aho-Corasick fast path. Terms containing non-ASCII
  characters keep the previous Unicode-aware matching. One accepted edge: with ASCII-only terms,
  a character whose Unicode lowercase folds into ASCII (such as 'İ') no longer matches.

## 1.0.2
- Updated dependencies.

## 1.0.1
- Removed emojis. They don't render properly on every terminal.

## 1.0.0
- Initial release.