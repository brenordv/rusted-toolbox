# eh-gui

A desktop window for Azure Event Hubs: connect to an event hub and capture its events into the
same local archive the `eh` CLI uses, watch them arrive in a live list, filter them, inspect
one, copy it, or export it to a file. Archive search and bulk export come in a later version;
until then `eh export` writes the archive out.

Screenshots: to come with the Export section.

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
- The data folder can change only while no session runs.

## Closing the window

Closing the window during a session stops the session and lets it finish writing message files
in the background; the app quits when it is done. If that fails, the window comes back with the
error (captured messages are safe in the archive, and `eh export` can write what was missed). If
it is still running after 30 seconds, the window comes back with a Force quit button.

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
selected), `seeded-search` (the same with a regex filter). The seeded Read states run no engine,
so Disconnect there never finishes. Debug builds accept Rune's agent flags
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
