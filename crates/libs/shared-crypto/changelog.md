# Changelog

## 1.0.0
- Initial release: X25519 identity generation and identity files (atomic
  create guard, owner-only permissions on Unix), recipient parsing from
  literals and files, streaming encrypt/decrypt over the age format (binary
  and ASCII armor, multi-recipient), passphrase-file refusal, the shared
  failure wording table, and the `output_name_for` default naming rule.
- `keys::write_identity` renders the standard identity-file lines to any
  writer; `save_identity_file` routes through it, and the seal CLI's
  keygen-to-stdout path is the other consumer.
- `output_name_for` strips the decrypt `.age` suffix ASCII
  case-insensitively, so `report.pdf.AGE` from a case-folding filesystem
  derives `report.pdf` instead of demanding an explicit output path.
