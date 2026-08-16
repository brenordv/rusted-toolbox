# 2.0.0
- Refactored to fit the new tooling
- Rewrote the formatting path as a byte-oriented chunk scanner: it handles non-UTF-8 input, preserves carriage returns, and runs in bounded memory regardless of line length.
- Buffered and locked stdout for the whole run, building each input chunk in a reusable buffer written in one call, so the formatting modes keep throughput close to a plain copy instead of a syscall per character.
- `-b` no longer prints spaces before a blank line; a blank line is now empty, matching `GNU cat`.
- `-E` prints `$` only when a newline is present, so an unterminated final line gets no `$`; `\r\n` renders as `^M$`.
- Line numbering and blank-line squeezing carry across files, so `cat -n a b` and `cat -s a b` number and squeeze continuously.
- A missing or unreadable file is reported, and the run continues with the remaining files, exiting non-zero at the end.
- A closed output pipe (`cat big.txt | head`) ends the run quietly with exit 0 instead of reporting a writing error. This diverges from `GNU cat`, which exits 141 from SIGPIPE; re-raising the signal is not portable to Windows.

# 1.0.1
- Updated dependencies.

# 1.0.0
- Initial release.