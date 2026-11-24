# seal-gui

The desktop companion to [seal](../tool-seal/readme.md): key-based file
encryption over the age format, in a window instead of a terminal.

Three tabs, switched from the header row or with `mod+1`, `mod+2`, and `mod+3`:

- **Text**: type or paste text, check recipients (or paste one key), and Seal
  produces ASCII armor ready for chat or email; Unseal takes armored text back
  with your identity file. The output sits in a selectable box with "Copy
  output". Unsealing something that is not text points you at the Files tab
  instead of showing garbage. `mod+enter` seals or unseals from any field.
- **Files**: drop files on the list from your file manager, or add them one at
  a time with "Add files…". Only regular files join: a dropped folder, pipe, or
  device is left out and the status line says why. One mode per run, Seal or
  Unseal; Unseal accepts only `.age` files and marks any other row before the
  run can start. Outputs land next to each input (`report.pdf` becomes
  `report.pdf.age`) or in a folder you choose. A row whose output exists is
  skipped unless "Overwrite existing files" is checked, and the row says so
  before you start. Every row shows its own result; one failed file never stops
  the rest, and the status line counts the batch and ends with a summary.
- **Keys**: generate a key pair (the identity file is written where you choose
  in the save dialog; only the public key comes back, with "Copy" and "Add to
  recipient book"). Picking a file that already exists asks once more, in the
  window, before replacing it. The recipient book holds named `age1...` keys
  the other two tabs offer as checkboxes; "Delete" asks once more ("Confirm
  delete") before an entry goes, and labels must be unique. The About group
  shows where the log file is.

Each tab has one status line at the bottom: what runs, what another tab runs,
or what your last action found. It stays until your next input on that tab.
Everything cryptographic runs through the same `shared-crypto` engine as the
CLI, on one background thread, so the window stays live during large files.
Files sealed here open with any age-compatible tool and the other way round.

## Security notes

- **There is no paste box for secret keys.** Unsealing takes the identity as
  a file you pick, never as text in a field: a secret in a text field ends up
  in clipboards, undo buffers, and screenshots.
- **The generated secret never reaches the window.** The worker writes the
  identity file (owner-only permissions on Unix) and only the public key comes
  back. It never writes over an existing identity file unless you confirm in
  the window, whatever the system's save dialog asked.
- **The config is trusted input.** Whoever can write `config.json` decides
  which key "Alice" means, so it lives in your user profile (see below), not
  the launch directory. An attacker who can already write to your profile is
  the compromised-machine case the threat model excludes.
- **Names from outside are shown cleaned.** File names and labels show control
  and right-to-left override characters as spaces, on screen and to screen
  readers.
- **Passphrase-encrypted age files are refused**, as in the CLI: seal is
  key-based only.
- What sealing does and does not protect (no signatures, names and sizes
  visible, plaintext stays on disk) is spelled out in the
  [seal readme](../tool-seal/readme.md); it applies here unchanged.

## Config

Convenience state is remembered in `config.json` under the per-user config
folder: `%APPDATA%\seal\` on Windows, `$XDG_CONFIG_HOME/seal/` (or
`~/.config/seal/`) elsewhere. It holds the recipient book (labels and public
keys), the last identity file, the last output folder, and the overwrite
choice. No field can hold a secret.

The file is saved whenever one of those changes, through a temp file and a
rename, so a crash never truncates the book. If a save fails (a read-only
profile, a full disk), a caption on every tab says so until a later save
works, and closing the window tries once more.

The file carries a `"tool": "seal"` marker. A `config.json` at that path whose
marker is missing, names another tool, or does not parse is left alone and
never overwritten: a caption says so, and changes in this session are not
saved until the file is moved or fixed.

## Logging

File logging is on by default, because a window has no visible terminal. The
log holds when each job starts and ends, file paths, and failure text; never
the text being sealed or unsealed, a key, or what was copied. The default
level is `info`. The Keys tab's About group shows the folder: `.seal-gui/logs/`
in your home folder.

The shared logging flags still apply:

- `--log-level <level>`: `trace` to `error`, or `disabled` to silence
  everything, the file included.
- `--log-to-console`: also log to stdout (useful when started from a shell).
- `--rotate-log-file-by-day`: date-stamped log files instead of one file.

## Jobs

One background thread runs one job at a time; a button that would start a
second one is disabled while one runs, and the status line says which (a Files
batch is one job, however many files it holds). If the thread ever stops, a
caption on every tab says so and the window needs a restart; nothing hangs.
Closing the window during a job lets that job finish before the program exits.

## Linux

File dialogs go through the XDG desktop portal (most desktops ship one; on a
minimal system install `xdg-desktop-portal` and a backend for your desktop).
Wayland delivers no dropped files to the window, so use "Add files…" there.

## Exit codes

| Code | Meaning                                    |
|------|--------------------------------------------|
| 0    | window closed normally                     |
| 1    | the window or the app could not be created |
| 2    | usage error (unknown flag)                 |

## Manual checklist (per release)

- Launch on each OS available, in light and dark appearance: the tabs switch
  by pointer and by `mod+1` to `mod+3`, and focus rings show on buttons and
  fields.
- Seal a text snippet, then unseal it; paste the armor into `rage` or `age`
  once as an interop check.
- Drop files from the file manager onto the Files list: the list lights up
  during the drag, rows land on the drop. Drop a folder: it is left out and
  the status line says why. Seal a mixed batch and read the summary.
- Unseal into a chosen folder.
- Generate a key pair to a new file, then onto the same file: the window asks
  before replacing it. Add the key to the book; copy and delete a recipient.
- Restart: the book, the identity file, the output folder, and the overwrite
  choice are as you left them.
- Make the config folder read-only and change something: the caption shows.
  Make it writable and change something else: the caption goes.
- Put another tool's `config.json` (any JSON without `"tool": "seal"`) in
  place: the caption shows and the file survives untouched.
- The log file exists after a run, in the folder the About group names.
