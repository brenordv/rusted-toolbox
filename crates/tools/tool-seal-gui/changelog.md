# Changelog

## 2.0.1
- The list box and the status badge come from `common-gui-rune` 1.4.0, where they moved with
  their tests; nothing on screen changes.

## 2.0.0
- Settings live in `~/.rusted-toolbox/seal-gui/config.json` (they lived in `%APPDATA%\seal\`
  on Windows and `$XDG_CONFIG_HOME/seal/` or `~/.config/seal/` elsewhere). On start seal-gui
  moves a settings file from the old folder when it carries seal's marker, and logs a warning
  naming both paths; a file there with another tool's marker is left alone, and one that cannot
  be read is left alone with a warning. When the file cannot be moved the old one stays in use
  and every start warns; when another program holds the old file open, so it cannot be removed
  after the move (Windows), the new one is used and every start warns until the old one is
  deleted; with files in both places the new one is used and every start warns.
  With no home folder nothing is saved. The settings folder is created owner-only on Linux and
  macOS.
- Log files go to `~/.rusted-toolbox/seal-gui/logs` (they went to `~/.seal-gui/logs`). The
  old folder is not moved; delete it once you no longer need its logs.
- seal-gui runs on Rune instead of egui and eframe; `common-gui` and `rfd` are no longer
  dependencies. The window keeps its title, size (980 x 720), and settings file; its smallest
  size is 760 x 680, the Text tab at rest with nothing to scroll.
- The tabs (Text, Files, Keys) sit in a header row; `mod+1`, `mod+2`, and `mod+3` switch
  between them, also while typing.
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
  "Unseal files", also on `mod+enter`. Each row shows a status badge, the path on one line (cut
  in the middle when it does not fit, with the whole path in a tooltip), the size, and Remove,
  with a line under the path when the row cannot run, failed, or would be skipped because its
  output exists. The list builds only the rows in view: with 2,000 files, a frame after a file
  event takes under 1 ms and a frame while scrolling about 1.5 ms. The status line counts the
  batch ("Sealing 2 of 3") and ends with a summary ("3 sealed, 0 skipped, 0 failed.");
  assistive technology hears "Sealing files" once instead of every count.
- Only regular files join the list. A dropped folder, pipe, or device is left out and the
  status line says why; a path that cannot be read joins with size 0 and its row fails with
  the real error when the batch runs. A dropped file joins whichever tab shows only when that
  tab is Files.
- Recipient selection follows the entry, not its place in the list, so two entries with the
  same label are chosen separately. An empty checkbox's edge and an empty radio's ring are
  drawn in the muted icon color (over 5:1 on the list), where the kit's own are 1.2 to 1.4:1,
  and a selected radio's dot in white.
- Keys tab: the recipient book (filling the window, with Copy and Delete on each row; Delete
  becomes "Confirm delete" once pressed), the add form under it (Label and Public key, added
  with "Add recipient", Return, or `mod+enter`; an empty or taken label or a bad key is named
  under its field), "Generate key pair" with an in-window confirm before an existing identity
  file is replaced, the generated public key with "Copy" and "Add to recipient book" (under the
  saved file's name, or into the form when that name is taken), and an About group with the
  log folder.
- The settings file is saved whenever the book, the identity file, the output folder, or the
  overwrite choice changes, instead of on tab switches and exit. A failed save shows a caption
  on every tab until a later save works, and closing the window tries once more; a foreign or
  unreadable `config.json`, or no settings folder at all, shows its own caption.
- Fixes that came with the rewrite: deleting a book entry no longer moves the checked boxes to
  other entries; "Add to recipient book" can no longer take a name from a failed key
  generation; a pipe or device can no longer join the Files list and hang the worker; Files
  rows have Remove and the list has "Clear list", so "Remove the invalid rows first" can be
  done.
- One background thread runs one job at a time. A job always ends with one answer; if the
  thread stops, the window says so instead of waiting. Closing the window during a job lets the
  job finish before the program exits, and the log says it is waiting.
- Logging boots through `common-cli`'s GUI boot: the log file is on, the default level is
  `info` (was `warn`), and a panic is logged. The log holds when a job starts and ends, file
  paths, and failure text; never the text being sealed or unsealed, a key, or a request.
- Generating a key pair never replaces an existing identity file without asking: the worker
  creates the file only if none is there, through `shared-crypto`'s new `create_identity_file`,
  and only the in-window confirm sends a replace.

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
