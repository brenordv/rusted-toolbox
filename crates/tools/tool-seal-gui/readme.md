# seal-gui

The desktop companion to [seal](../tool-seal/readme.md): key-based file
encryption over the age format, in a window instead of a terminal.

![screenshot placeholder](screenshot.png)

Three tabs:

- **Text**: paste text, pick recipients, and Seal produces ASCII armor ready
  for chat or email; Unseal takes armored text back to plaintext with your
  identity file. The output box is read-only with a Copy button. Unsealing
  something that isn't text (a pasted armor of a binary file) points you at
  the Files tab instead of rendering mojibake.
- **Files**: add files through a dialog or drag them from your file manager
  (the drop zone highlights during the drag; folders are rejected, duplicates
  ignored). One mode per run, Seal or Unseal; Unseal only accepts `.age`
  files and marks anything else invalid before the run can start. Outputs
  land next to each input (`report.pdf` becomes `report.pdf.age`) or in a
  folder you choose. A row whose output already exists is skipped unless
  "overwrite existing" is checked. Every row shows its own result; one failed
  file never stops the rest, and a toast summarizes the batch.
- **Keys**: generate a key pair (the identity file is saved through a native
  save dialog; only the public key is shown, with Copy and add-to-book
  buttons) and keep a recipient book: named `age1...` keys the other two tabs
  offer as checkboxes. Deleting a book entry takes two clicks on the same
  button; duplicate labels are rejected.

Everything cryptographic runs through the same `shared-crypto` engine as the
CLI, on a background worker thread, so the window stays live during large
files. Files sealed here open with any age-compatible tool and vice versa.

## Security notes

- **There is no paste box for secret keys.** Unsealing takes the identity as
  a file you pick, never as text in a widget: a secret in a GUI text field
  ends up in clipboards, undo buffers, and screenshots. Nothing in this app
  reads secret material through a widget.
- **The generated secret never reaches the UI.** Keygen writes the identity
  file inside the worker (owner-only permissions on Unix) and only the public
  key comes back to the window.
- **The config is trusted input.** Whoever can write `config.json` decides
  which key "Alice" means, so it lives in your user profile (see below), not
  the launch directory; a folder you happen to start the app from cannot
  plant a poisoned recipient book. An attacker who can already write to your
  profile is the compromised-machine case the threat model excludes.
- **Passphrase-encrypted age files are refused**, same as the CLI: seal is
  key-based only.
- What sealing does and does not protect (no signatures, names and sizes
  visible, plaintext stays on disk) is spelled out in the
  [seal readme](../tool-seal/readme.md); it applies here unchanged.

## Config

Convenience state is remembered in `config.json` under the per-user OS config
directory: `%APPDATA%\seal\` on Windows, `$XDG_CONFIG_HOME/seal/` (or
`~/.config/seal/`) elsewhere. It holds the recipient book (labels and public
keys), the last identity file path, the last output folder, and the overwrite
checkbox. No field can hold a secret.

The file carries a `"tool": "seal"` marker. A `config.json` at that path
whose marker is missing, names another tool, or does not parse is ignored and
never overwritten: the status bar says so, and the recipient book stays
read-only until the file is moved or fixed. Saves happen on tab switches and
on exit, through a temp file and rename, so a crash never truncates the book.
If a save fails (read-only profile, full disk), you get one toast and a log
entry rather than silence or a toast on every tab switch.

## Logging

One deliberate deviation from the CLI tools: **file logging is on by
default**, because a windowed app has no visible stderr. Events at `warn` and
above land in `~/.seal-gui/logs/seal-gui.log` (hover the `logs` label in the
status bar for the exact folder on your machine). Job failures, per-file
failures included, are logged as errors, so "why did sealing fail last night"
is answerable from that file.

The shared logging flags still apply:

- `--log-level <level>`: `trace` to `error`, or `disabled` to silence
  everything, the file sink included.
- `--log-to-console`: also log to stdout (useful when started from a shell).
- `--rotate-log-file-by-day`: date-stamped log files instead of one file.

## Jobs

One background worker runs one job at a time; buttons that would start a
second job are greyed out while one is in flight (a Files batch is one job,
however many files it holds). If the worker ever dies, the status bar says so
and the app needs a restart; nothing hangs.

## Linux

File dialogs come from `rfd`, which talks to the XDG desktop portal at
runtime (most desktops ship one) and falls back to zenity. On a minimal
environment without either, install `xdg-desktop-portal` plus a backend for
your desktop, or `zenity`.

## Exit codes

| Code | Meaning                                    |
|------|--------------------------------------------|
| 0    | window closed normally                     |
| 1    | the window or the app could not be created |
| 2    | usage error (unknown flag)                 |

## Manual checklist (per release)

- Launch on each OS available: the dark theme renders, tabs switch.
- Drag files from the OS file manager: the drop zone highlights during the
  drag, rows land on the drop.
- Seal a text snippet, then unseal it; paste the armor into `rage` or `age`
  once as an interop spot-check.
- Generate a key pair: cancel does nothing, saving over an existing file asks
  first, the toast names the saved path.
- Plant a foreign `config.json` (any JSON without `"tool": "seal"`): the
  status-bar note appears and the file survives untouched.
- An idle window sits at zero CPU (the toast timer must not busy-repaint).
- The log file exists after a run and its folder shows on the `logs` tooltip.
