# Changelog

## 1.0.0
- Initial release: `keygen` (identity file with atomic overwrite guard, public key on
  stdout, or the full identity to stdout with a stderr notice), `encrypt` (repeatable
  `-r` literals and `-R` files, `--armor`, file or stdin input), and `decrypt`
  (repeatable `-i` identity files, poisoned output removed on failure).
- Default naming via the shared rule: encrypt appends `.age` to the whole file name,
  decrypt strips exactly one; `-o` overrides, `-o -` forces stdout.
- One overwrite rule: an existing output path is refused without `--force`; writes go
  through a temp file renamed into place.
- Binary ciphertext aimed at a terminal is refused; armored output and plaintext are
  allowed through.
- Exit codes 0/1/130, usage errors 2; a consumer closing the output pipe ends the run
  with exit 0.
- `watch` subcommand implemented: watches a folder (top level only) and seals every
  file that appears or changes into the safe folder, replacing previous outputs
  atomically. Public-key only, like the rest of the tool.
- Startup arms the watcher first, then one sweep seals every file whose output is
  missing or older than its source, catching changes made while the tool was down.
- Temp-artifact names (`~$` prefix; `.tmp`, `.part`, `.crdownload`, `.swp` suffixes),
  dotfiles, and `.age` files are skipped; a locked source is retried 5 times with
  doubling backoff from 100 ms, then given up until its next change event.
- The watch and safe folders are refused when one resolves inside the other; the
  check compares canonicalized paths and repeats before every recovery sweep.
- Watcher errors that may have lost events re-check confinement and re-sweep; a dead
  watcher or vanished watch folder ends the run with exit 1 after the summary prints.