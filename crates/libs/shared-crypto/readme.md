# shared-crypto

The encryption engine behind the `seal` CLI and GUI. Wraps the `age` crate
(X25519 key agreement, ChaCha20-Poly1305 in 64 KiB authenticated chunks, the
published age format) behind a small API so the consuming tools never import
`age` directly:

- `keys::generate_identity()` / `keys::public_key_of(&Identity)`: a fresh
  X25519 identity; its public key in the `age1...` form.
- `keys::save_identity_file(&Identity, &Path, overwrite)`: writes the
  standard age identity file (`# created:` timestamp, `# public key:` line,
  the `AGE-SECRET-KEY-1...` line). Without `overwrite` the existence guard
  is atomic (`create_new`). Owner-only on Unix (0o600 file, 0o700 created
  parents); on Windows the file inherits the parent's ACL, so keep identity
  files under your user profile.
- `keys::load_identities(&[PathBuf])`: parses identity files; blank lines
  and `#` comments are ignored, and a malformed line is reported by file and
  line number, never by content.
- `keys::parse_recipients(&[String], &[PathBuf])`: `age1...` literals plus
  recipient files (same comment rules), literals first; an empty combined
  result is an error, not an empty vec.
- `engine::encrypt(&[Recipient], &mut Read, &mut Write, armor)`: streaming
  multi-recipient encrypt, binary age format or ASCII armor. Returns the
  plaintext byte count; stat the output for the ciphertext size.
- `engine::decrypt(&[Identity], &mut Read, &mut Write)`: streaming decrypt,
  armored and binary inputs auto-detected (CRLF-rewritten armor included).
  Returns the plaintext byte count.
- `output_name_for(&Path, Direction)`: the default name mapping both tools
  share. Encrypt appends `.age` to the whole file name (`x.tar.gz` becomes
  `x.tar.gz.age`); decrypt strips exactly one `.age` and fails (asking for
  an explicit output) on any other name.
- `errors`: the shared failure wording as message constants and builders.
  No public error enum; everything is `anyhow::Result` with context, per the
  workspace convention for internal libs.

Re-exported for consumers: `Identity`, `Recipient` (x25519), `SecretString`.
Anything else from `age` appearing outside this crate is a review defect.

## Contracts

- Any `Err` from `decrypt` poisons the output: plaintext may already have
  streamed out before the corruption was detected, so discard whatever the
  output received on any failure.
- Passphrase-encrypted (scrypt) age files are detected and refused before
  any key or payload work; this engine is key-based only.
- Secret keys only exist as `SecretString`. Nothing here logs, prints, or
  serializes one, no type holding one implements `Serialize`, and failure
  messages never echo key material (a malformed identity line is reported by
  line number only).
- `encrypt` writes the age header before the first payload byte, so a failed
  call can leave a partial file; callers writing to a path should write to a
  temp file and rename.

## Threat model

What this protects: confidentiality and integrity of file contents against
anyone who has the ciphertext but no matching identity. Chunk-level
authenticated encryption means truncation and tampering are detected as
decryption errors.

What it does not protect: sender authenticity (age does not sign; decryption
proves the file was sealed to your key and arrived intact, not who sent it);
file names and sizes; plaintext that remains on disk (encrypting does not
delete or shred the source); a compromised machine that can read identity
files.
