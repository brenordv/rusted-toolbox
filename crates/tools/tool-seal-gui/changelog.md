# Changelog

## 1.0.0
- Initial release.
- Text tab: seal text to recipients as ASCII armor, unseal armored text with
  an identity file, read-only output with a Copy button. Non-text plaintext
  redirects to the Files tab instead of rendering mojibake; any unseal
  failure clears the output box (poisoned-output rule).
- Files tab: add files via dialog or drag-and-drop (directories rejected,
  duplicates ignored, hover highlight), one Seal/Unseal mode per run with
  invalid-row marking, outputs next to each input or into a chosen folder,
  overwrite gate with per-row skip, per-row progress and result badges, batch
  summary toast. One failed file never aborts the rest.
- Keys tab: key-pair generation through a native save dialog (the secret is
  generated, written, and dropped inside the worker; only the public key is
  displayed), recipient book with two-click delete confirm and
  duplicate-label rejection.
- `config.json` in the per-user OS config dir, guarded by the
  `"tool": "seal"` marker: foreign or unreadable files are ignored, never
  overwritten, and reported in the status bar; writes are atomic
  (temp file + rename) and flushed on tab switch and exit; a failed save
  toasts once per session.
- No paste box for secret keys anywhere: identities enter as files only.
- Dark theme and widget recipes from `common-gui`; single background worker
  with bounded per-frame event drain, repaint wake after every event, clean
  shutdown on window close, and a "worker stopped" state instead of a silent
  freeze.
- File logging on by default at `warn` (`~/.seal-gui/logs/seal-gui.log`);
  job failures and per-file failures always reach the log file.
