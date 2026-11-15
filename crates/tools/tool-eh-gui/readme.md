# eh-gui

A desktop window for Azure Event Hubs: connect to an event hub and capture its events into the
same local archive the `eh` CLI uses, watch them arrive in a live list, filter them, inspect
one, copy it, or export it to a file. The Export section searches the archive by time and
text and exports one message or every match, with or without a connection.

Screenshots come in a later version.

```
eh-gui                      # opens the window
eh-gui --log-level debug    # more detail in the log file
```

The only flags are the shared logging flags (`--log-level`, `--log-to-console`,
`--log-to-file`, `--rotate-log-file-by-day`, `--app-header`). Everything else happens in the
window.

## Connecting

Press Connect to open the connection sheet. Its fields scroll between the title and the
buttons, so Connect stays on screen in a small window. Escape, the close button, or a click
outside it closes the sheet. It holds:

- **Sign-in method**: a connection string, an Azure credential (developer tools, managed
  identity, or a client secret) with the namespace and event hub, or a connection string kept in
  Key Vault (the vault URL and secret name, plus the credential that reads it).
- **Storage**: the data folder (Change picks another one), and whether captured messages are
  also written as text files (Off, Immediate as they arrive, or At end of the session).

Connect checks the fields first (namespace and event hub names use the same rules as `eh`).
Secrets are typed into hidden fields with a show toggle; they are wiped after Connect and Save
config, and never shown to screen readers.

## Reading

While a session captures, the Read section shows:

- **Statistics**, refreshed once a second: received, rate, stored, duplicated, filtered out,
  peak rate, time since the last message, and elapsed time. Press Received for the counts per
  partition. A "UI dropped" tile appears when the list skipped messages under load; the
  archive and the counts still include them. "Last message" turns the warning color after 30 seconds
  without one (a quiet hub). When the engine stops sending updates altogether for three
  feedback intervals, the connection pill reads "Not receiving updates".
- **The message list**: the newest 5,000 messages (fewer with very large bodies: the list keeps
  at most 256 MB of bodies). Older ones stay in the archive. New rows land up to four times a
  second. While you are at the bottom the list follows new messages; scroll up and it stays
  put, with a "↓ N new" pill that brings you back. Up, Down, Home, and End move the selection,
  Escape clears it, and Ctrl+C (Cmd+C on macOS) copies the selected message's body.
- **Filter rows** (Ctrl+F / Cmd+F): an ephemeral filter over the list, by text or, with `.*`,
  a regular expression (case-insensitive, at most 1,000 characters). It looks at the whole
  body, including what the preview cuts off, and the message and correlation ids. It changes
  nothing captured or stored. Under a sustained burst a filter can trail the newest rows by a moment.
- **The inspector**, when a message is selected (its close button or Escape in the list closes
  it): its metadata and properties, its body (the
  first 256 KB; Copy body copies all of it), Copy as JSON, and Export, which writes that one
  message as txt, csv, or json to a file you pick (with csv, mind spreadsheet apps that run
  cells starting with `=`, `+`, `-`, or `@` as formulas). Exporting one message does not count
  in the archive's export log.
- **The session log**, in the connection pill's popover: partition errors and ends, with a
  count on the pill until you open it.

Copied bodies go to the system clipboard as plain text. On Windows, clipboard history and cloud
clipboard sync keep copies of them, which matters for payloads holding personal data.

### Capture rules

The sidebar's capture rules decide which captured messages the list shows and the message
files hold (the archive stores every message either way). A time rule takes an RFC3339
timestamp or a date (`2026-07-02` is midnight UTC; During needs a full timestamp, as
`eh read --during` does); a text rule takes comma-separated keywords or a regular expression.
Apply rules sends them to the running session; rows already in the list stay. While no session
runs, applied rules are kept for the next Connect, and Save config writes them to
`config.json`. A recalled `config.json` fills them in at launch.

## Exporting

The Export section works on the data folder's archive, connected or not, and while a session
captures into it.

- **Search archive** (sidebar): the same time and text fields as the capture rules. Search shows
  every archived message that matches, oldest enqueue time first (messages without an enqueue
  time come first, and only in a search without a time condition). It is disabled while a page
  is still being read: a search cannot be stopped halfway, so a slow regex over a large archive
  has to finish first. Clear empties the form and keeps the results.
- **Results** load 250 at a time as you scroll toward the end, up to 5,000 rows (fewer with very
  large bodies: at most 256 MB of bodies; loading a page briefly holds one more page on top).
  Past that the footer says so; narrow the search, or export them all. A page that fails to load
  shows its error with Retry. Each page is read on its own, so a message overwritten between two
  pages (a capture with `ignore_checkpoint` running at the same time) can be missing.
- **The inspector** works as in the Read section: metadata, body, Copy body, Copy as JSON, and
  Export of one message to a file you pick.
- **Export all matches** (the bar at the bottom): exports every match of the last search, not
  just the loaded rows, and not what the form says now if you edited it since. Options sets the
  format (txt, csv, or json; with csv, mind spreadsheet apps that run cells starting with `=`,
  `+`, `-`, or `@` as formulas), metadata, one file per month, local time, skip already exported,
  and the folder (the data folder's `exports` unless you pick another; picks are not saved to
  `config.json`). While it runs the bar shows exported, skipped, and failed counts and Cancel,
  which stops between messages, so every file written stays complete. When it ends the bar
  shows the counts, and when files failed, the first failure (the log file has the rest).
- **Skip already exported** uses the archive's export log, which is global: a message exported
  once, in any format or folder, is skipped, unless the exported file no longer exists (with one
  file per month it is skipped even then). An export of one message from the inspector does not
  count.
- A bulk export during a live capture is fine: messages captured while it runs may or may not be
  included, and the counts say what was written.
- One bulk export runs at a time, and the data folder cannot change while it runs.

The CSV injection warning and how the archive grows (there is no purge yet) are covered in
`shared-eventhub-persistence`'s readme.

## Config

At launch eh-gui reads `config.json` from the working directory, exactly as `eh` does, and fills
the sheet from it. A secret the config keeps in the data folder's secrets file shows as "Saved
in local store" and is read only when you connect. A `config.json` that belongs to another tool
is left alone, and the sheet says so. Save config writes `config.json` with the CLI's rules; keep
one `config.json` per data folder.

## Data folder and the archive

- One data folder holds the archive of one event hub. Connecting to another event hub with the
  same data folder fails and names both hubs; use another data folder.
- The archive is locked while eh-gui uses it: from the first Connect until the app quits (or the
  data folder changes), `eh read` and `eh export` on the same folder report it as in use. One
  process at a time per data folder.
- The data folder can change only while no session and no bulk export runs. A new data folder
  clears the Export section's results.
- Secrets saved with Save config go to the data folder's secrets file, which is obfuscated, not
  encrypted; protect the folder like the secrets themselves.
- Data captured by `eh-read` 1.1.0 (sled) is not migrated; the last released 1.1.0 binaries
  still read it.

## Closing the window

Closing the window during a session stops the session and lets it finish writing message files
in the background; the app quits when it is done. If that fails, the window comes back with the
error (captured messages are safe in the archive, and `eh export` can write what was missed). If
it is still running after 30 seconds, the window comes back with a Force quit button.

Closing the window during a bulk export cancels it; the app quits once the files being written
are complete. If the export failed (some files could not be written, or the archive could not
be read), the window comes back to show it.

## GPU

eh-gui draws with the GPU: DirectX 12 on Windows, Vulkan on Linux, Metal on macOS. A Windows
machine without a hardware adapter falls back to the WARP software renderer (slow, but it
works). When no window can be created at all, eh-gui logs why and exits with code 1; use the
`eh` CLI there.

## Window

- Windows and Linux keep the system title bar; on macOS the toolbar is the title bar.
- The window can't be made smaller than 960x600.
- Colors follow the system's light or dark setting while the app runs. On Windows eh-gui reads
  the setting itself once a second, so a switch shows within about a second; the title bar's
  own color is the system's.

## Logging and diagnosing

eh-gui logs to a file by default, at info, because a window has no visible console. The file is
`~/.eh-gui/logs/eh-gui.log`; the connection pill's popover shows the exact path, or why file
logging is unavailable. `RUST_LOG` overrides the level (`RUST_LOG=warn` writes a sparse file),
and `--log-level disabled` turns logging off.

To look at rendering cost, run from a terminal with `RUNE_PERF_WARN=1 RUNE_COMMIT_BUDGET_MS=8`
and `--log-level debug`; Rune prints every commit over the 8 ms budget on the terminal.

## Demo mode (development)

A dev-only binary runs the real window against a generated event hub and a temporary data folder
(deleted on exit), with no namespace or credentials. Pick a scenario with `EH_GUI_DEMO` (default
`live`): `live`, `burst`, `quiet`, `partition-fail`, `start-fail`, `slow-finish`,
`seeded-empty`, `seeded-failed`, `seeded-connect`, `seeded-read` (a full list with a message
selected), `seeded-search` (the same with a regex filter), `seeded-export` (the Export section
over an archive of 6,000 generated messages, searched for everything; paging and bulk exports
are real). The seeded Read states run no engine, so Disconnect there never finishes. Demo mode
reads and writes only under its temporary folder: bulk exports go to its `data/exports`,
whatever folder is picked. Debug builds accept Rune's agent flags
(`--snap`, `--script`, `--tree`, `--trace`, `--scene`); write their output under
`target/eh-gui-demo/`, which git ignores.

```
EH_GUI_DEMO=burst cargo run -p eh-gui --features demo --bin eh-gui-demo
EH_GUI_DEMO=seeded-failed cargo run -p eh-gui --features demo --bin eh-gui-demo -- --snap target/eh-gui-demo/failed.png
```

Release builds of the workspace leave the demo out.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Quit normally |
| 1 | The window could not start, or the app failed to boot |
| 2 | Invalid command-line flags |

## Credits

- [Rune](https://github.com/anomalyco/rune) (MIT), which embeds the Inter and IBM Plex Mono
  fonts (SIL Open Font License).
- Icons from [Lucide](https://lucide.dev) (ISC; `assets/icons/LICENSE`): panel-left, plug,
  unplug, radio-tower, archive, folder-open.

## Manual checklist (per release)

- Launch on each available OS; switch light and dark while running (colors ease); turn reduced
  motion on.
- Connect with a bad namespace: Failed shows, with a toast.
- Close the window during a session with message files set to "At end": the
  window leaves, the process finishes and quits, and the log shows both lines.
- Force a failure of the at-end pass: the window comes back with the error.
- The log path in the pill's popover names the file being written.
- Live hub: change the rules mid-run (Filtered out climbs); follow the tail under load; scroll
  up and watch the pill; filter rows while messages stream; copy a body into an editor; export
  one message to a chosen folder, over an existing file (the save dialog asks first).
- Cut the network: "Not receiving updates" shows, and clears when updates resume; a quiet hub
  only turns Last message the warning color.
- Run with `RUNE_COMMIT_BUDGET_MS=8` under `EH_GUI_DEMO=burst`, and while opening and closing
  the inspector: no commit over budget.
- Export section on a real archive: search by a time window plus a regex, scroll through several
  pages, bulk export a few thousand messages and cancel halfway (the partial files parse), run
  it again with Skip already exported on (the skips show), and run a bulk export while a live
  capture writes into the same archive.
