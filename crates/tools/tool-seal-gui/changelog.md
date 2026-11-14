# Changelog

## Unreleased
- Builds on Rune's forks of `winit`, `wgpu-hal`, and `accesskit_windows`: the workspace now
  depends on Rune (for `eh-gui`), and its `[patch.crates-io]` applies to every crate here.
  Rune is pinned at rev `c4eda05889c984fff91ccd906e3c00cdfc4cae24`. Each fork was checked to be
  byte-identical to its crates.io release plus Rune's committed `vendor/<name>.patch`, and each
  patch was read by hand: no network, file, process, or telemetry code.
- What seal-gui can notice:
  - winit 0.30.13: on Wayland, touchpad scroll distances are 12 times larger. On Windows, new
    windows skip a taskbar COM call and are created at their requested position.
  - wgpu-hal drops from 30.0.2 to 30.0.1 (Rune's fork). eframe renders with wgpu, so on DX12
    the swap chain is `FLIP_SEQUENTIAL` instead of `FLIP_DISCARD`, and the shader compiler
    (FXC) loads on the first shader compile instead of at startup.
  - accesskit_windows 0.35.2: headings and landmarks report their level and type to UI
    Automation.

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
