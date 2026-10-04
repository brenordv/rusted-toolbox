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
- `watch` is a reserved stub that exits 1 with a planned-for-later note.
