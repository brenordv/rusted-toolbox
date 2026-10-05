# seal

`seal` encrypts and decrypts files with public-key cryptography: generate an identity
once, hand the `age1...` public key to anyone who should be able to send you files, and
open what they seal with your identity file. No shared passwords, nothing to agree on
in advance beyond the public key.

seal encrypts with 256-bit authenticated encryption: X25519 key agreement and
ChaCha20-Poly1305, the same cryptography that protects TLS 1.3 and WireGuard traffic.
It implements the age format, a published specification with interoperable
implementations, so files sealed here open with any age-compatible tool.

## Command line examples

### Generate an identity
```bash
$ seal keygen -o key.txt
age1example0000000000000000000000000000000000000000000000000
```
The identity file is written owner-only on Unix (0600); the public key, the one line on
stdout, is what you share. Replacing an existing file needs `--force`.

Running `seal keygen` without `-o` prints the whole identity file (secret key included)
to stdout, with a warning on stderr. Prefer `-o`: a shell redirect creates the file with
your umask's default permissions, not 0600.

### Encrypt a file
```bash
$ seal encrypt -r age1example000... report.pdf
sealed report.pdf -> report.pdf.age (1.20 MB)
```
`-r` takes a public key literally and repeats (`-r a... -r b...`); `-R team.txt` reads
one recipient per line (blank lines and `#` comments ignored). Every listed recipient
can open the result.

### Decrypt it
```bash
$ seal decrypt -i key.txt report.pdf.age
opened report.pdf.age -> report.pdf (1.19 MB)
```
`-i` repeats too; the first identity that matches wins.

### Stream through pipes
```bash
$ tar cz docs | seal encrypt -R team.txt -o docs.tar.gz.age
$ seal decrypt -i key.txt -o - report.pdf.age | less
```
Stdin input and stdout output need no flags; `-o -` forces stdout for a file input.

## Where output goes

| Case                                  | Default output                                     |
|---------------------------------------|----------------------------------------------------|
| `encrypt file.txt`                    | `file.txt.age` next to the input                   |
| `encrypt file.txt --armor`            | `file.txt.age` (armored contents, same name)       |
| `encrypt` from stdin                  | stdout                                             |
| `decrypt file.txt.age`                | `file.txt` (strips exactly one `.age`)             |
| `decrypt file.enc` (no `.age` suffix) | error: pass `-o <output>`                          |
| `decrypt` from stdin                  | stdout                                             |

- An explicit `-o` always wins; `-o -` forces stdout.
- An output path that already exists is refused without `--force`, the derived `.age`
  default included.
- Output files are written through a temp file in the destination directory and renamed
  into place on success, so a failed run never leaves a half-written output and a
  `--force` overwrite is atomic.
- Binary ciphertext aimed at a terminal is refused ("use --armor or redirect");
  armored output and decrypted plaintext may go to a terminal.

## Piped output and exit codes

When decrypt streams to stdout, bytes already written cannot be recalled if corruption
is detected mid-stream. **Piped output is only trustworthy when the exit code is 0.**
A consumer that stops reading early (`seal ... | head`) ends the run quietly with
exit 0.

| Code | Meaning |
|------|---------|
| 0 | Success (including the consumer closing the pipe early) |
| 1 | Any failure: bad key material, corrupted input, refused overwrite, I/O error |
| 2 | Usage error rejected by the argument parser |
| 130 | Interrupted with Ctrl+C |

On failure, any partially decrypted file output has already been removed; discard piped
output on any non-zero exit.

## Watch mode

`seal watch` keeps a folder sealed: every file that appears or changes in the watch
folder is encrypted to `<name>.age` in the safe folder, replacing any previous version
there. It runs until Ctrl+C and needs no secret material; the watching machine only
ever holds public keys.

```bash
$ seal watch -r age1example000... --watch-dir inbox --safe-dir sealed
watching inbox -> sealed (1 recipient(s))
swept 2 file(s): 1 encrypted, 1 up to date, 0 failed
sealed report.pdf -> sealed/report.pdf.age (1.20 MB)
```

`-r` and `-R` work exactly as in `encrypt`. On startup the watcher arms first, then one
sweep seals every file whose output is missing or older than its source, so files added
or changed while the tool was down are caught up. After that, change events drive the
work.

| Rule         | Behavior                                                                                           |
|--------------|----------------------------------------------------------------------------------------------------|
| Scope        | Top level of the watch folder only; subfolders, symlinks, and other non-files ignored              |
| Temp names   | `~$` prefix, or a `.tmp`, `.part`, `.crdownload`, `.swp` suffix: skipped                           |
| Hidden files | Dotfiles are skipped                                                                               |
| `.age` files | Skipped, so ciphertext dropped into the watch folder is never re-sealed                            |
| Overwrites   | An existing output is replaced atomically (temp file + rename), no `--force` needed                |
| Locked files | The open is retried 5 times with doubling backoff from 100 ms, then left for its next change event |

Events are debounced for 2 seconds so editors' write-then-rename save dances settle
before sealing; the watcher assumes writers finish within that window. A writer that
stalls longer can get a partial file sealed, and the next change event replaces it with
the complete one.

Watcher errors that may have lost events trigger a confinement re-check and a fresh
sweep. If the watcher itself dies or the watch folder disappears, the run prints the
reason and the summary, then exits 1: a watcher that watches nothing must not keep
running. Ctrl+C finishes the in-flight file, prints the summary, and exits 130.

Caveats:
- The two folders must not contain each other. The check compares resolved
  (junction/symlink-free) paths, at startup and before every recovery sweep.
- Plaintext stays in the watch folder; sources are never deleted. Back up or sync the
  safe folder and treat the watch folder as the sensitive one.
- Network filesystems may deliver no change events; watch a local folder (for synced
  folders, watch the local replica).
- The safe folder is a mirror, not a history: a corrupted source that still triggers an
  event replaces the last good ciphertext.

## Notes
- Passphrase-encrypted age files are refused with a clear message; seal is key-based
  only.
- Failure messages never echo key material; a malformed identity line is reported by
  file and line number only.
- Shared flags from the common CLI: `--app-header`, `--verbose`, `--log-level <level>`
  (case-insensitive), `--log-to-console`, `--log-to-file`, `--rotate-log-file-by-day`.

## Threat model

What this protects: confidentiality and integrity of file contents against anyone who
has the ciphertext but no matching identity. Every 64 KiB chunk is authenticated, so
truncation and tampering surface as decryption errors, never as silently wrong
plaintext.

What it does not protect: sender authenticity (age does not sign; a successful decrypt
proves the file was sealed to your key and arrived intact, not who sent it); file names
and sizes (`file.txt.age` mirrors the plaintext name); plaintext that stays on disk
(encrypting does not delete or shred the source); a compromised machine that can read
your identity files.