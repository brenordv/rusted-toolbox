# Changelog

## 2.0.0
- seal-gui runs on Rune instead of egui and eframe; `common-gui` and `rfd` are no longer
  dependencies. The window keeps its title, size (980 x 720), and settings file; its smallest
  size is 760 x 680, the Text tab at rest with nothing to scroll.
- The tabs (Text, Files, Keys) sit in a header row; `mod+1`, `mod+2`, and `mod+3` switch
  between them, also while typing. The Keys tab says it arrives with the next change.
- Text tab: Direction (Seal or Unseal), the input, the recipients (each saved one as a
  checkbox row with its shortened key, plus an Other key field) or the identity file with
  "Choose identity file…", the output in a selectable box with "Copy output", and Seal or
  Unseal at the bottom. Return types a line break in the input; `mod+enter` starts the job from
  any field.
- No toasts and no status bar. Each tab's status line says what runs, what another tab runs
  ("Text: sealing…"), or what the last action found or copied; it stays until your next input
  on that tab. A press that cannot start says what is missing, marks the field, and moves
  focus to it; Seal stays enabled for that. A stopped worker shows a standing caption on every
  tab.
- Files tab: Mode (Seal or Unseal) with "Overwrite existing files" beside it; the file list,
  which takes files dropped on it (lit with the accent while a drag hovers) and grows with the
  window; "Add files…" (one file per dialog) and "Clear list"; the recipients or the identity
  file as on the Text tab; Output (next to each input, or a chosen folder); and "Seal files" or
  "Unseal files", also on `mod+enter`. Each row shows a status badge, the path, the size, and
  Remove, with a line under the path when the row cannot run, failed, or would be skipped
  because its output exists. The status line counts the batch ("Sealing 2 of 3") and ends with
  a summary ("3 sealed, 0 skipped, 0 failed."); assistive technology hears "Sealing files" once
  instead of every count.
- Only regular files join the list. A dropped folder, pipe, or device is left out and the
  status line says why; a path that cannot be read joins with size 0 and its row fails with
  the real error when the batch runs. A dropped file joins whichever tab shows only when that
  tab is Files.
- Recipient selection follows the entry, not its place in the list, so two entries with the
  same label are chosen separately. An empty checkbox's edge and an empty radio's ring are
  drawn in the muted icon color (over 5:1 on the list), where the kit's own are 1.2 to 1.4:1,
  and a selected radio's dot in white.
- One background thread runs one job at a time. A job always ends with one answer; if the
  thread stops, the window says so instead of waiting. Closing the window during a job lets the
  job finish before the program exits, and the log says it is waiting.
- Logging boots through `common-cli`'s GUI boot: the log file is on, the default level is
  `info` (was `warn`), and a panic is logged. The log holds when a job starts and ends, file
  paths, and failure text; never the text being sealed or unsealed, a key, or a request.
- Generating a key pair (wired in the Keys tab next) never replaces an existing identity file
  without asking: the worker creates the file only if none is there, through `shared-crypto`'s
  new `create_identity_file`.

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
