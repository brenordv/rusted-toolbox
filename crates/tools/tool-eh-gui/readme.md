# eh-gui

A desktop window for Azure Event Hubs: connect to an event hub and capture its events into the
same local archive the `eh` CLI uses, watch them arrive in a live list, filter them, inspect
one, copy it, or export it to a file. The Export section searches the archive by time and
text and exports one message or every match, with or without a connection. The Send section
publishes test events to an event hub, batched or one every N milliseconds.

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

## Sending

The Send section publishes one body a given number of times to the event hub the connection
sheet names, with or without a capture session, so you can capture in Read and watch your own
test events arrive. Sending writes to a real event hub: the sidebar and the line next to Send
name it.

- **Destination** (sidebar): the namespace and event hub the sheet resolves to, the same way the
  open will (a connection string's `EntityPath` or the hub typed in the sheet; a mismatch reads
  "Not a valid connection string"). A saved connection string or a Key Vault secret shows "the
  hub in the saved connection string" until a send resolves it. Edit… opens the sheet to choose
  where to send: its footer has Save config and Done, never Connect. While a session captures,
  Send goes to the hub being read and Edit… waits for Disconnect; while a send runs, the hub
  can't change.
- **Routing**: Automatic (the service spreads events over partitions) or Partition key (every
  event carries the key, so they land in one partition, in order).
- **Message**: content type (optional), count (1 to 1,000,000), and pacing. Batched packs as
  many events per batch as fit and sends batch after batch. Paced sends one event per batch,
  each send starting the interval (1 to 3,600,000 ms) after the previous one started; a slow
  send delays the next one, never a burst to catch up, and throttling or SDK retries stretch the
  interval. The first paced event goes at once. Count and pacing are per send and never saved to
  `config.json`, as with `eh send --interval-ms`.
- **Event body**: a monospace editor; Return types a newline. Bodies over 1 MiB don't fit the
  editor (use `eh send --body-file`). A body that fits the editor can still be too large for one
  batch: the SDK limits a single event to about half the event hub's publish limit (about 128 KB
  on Basic, 512 KB on Standard and Premium), and the send then fails with that message.
- **Template** (the switch beside "Event body"): the body becomes a template, and every
  `{{type option=value}}` placeholder gets a fresh mock value in each event, and `{{now}}` and
  `{{utc-now}}` the time the event is built, as with `eh send --template` (the `eh` readme has
  the syntax and its limits). "Show placeholders" lists the types, the clock placeholders, and
  the options. Under the editor a Sample block shows one render, with the time of that render;
  Regenerate draws another. While the template does not compile, the block says where and why,
  and a Send press marks the editor and blocks the send until it is fixed. A sample that fails to render (a value
  too large for 1 MiB, say) shows its error but does not block Send; the send then fails at its
  first event. Values go in unescaped, so a JSON template can break on a value holding `"`.
- **Send** (Ctrl+Enter or Cmd+Enter from any Send field). The line beside it shows the run:
  connecting, then the sent count, the pacing, and the elapsed time; Cancel stops before the next
  batch or during a paced wait within about a tenth of a second. A batch already handed to the
  SDK finishes first, which can take up to about a minute while the SDK retries a throttled
  batch; the window gives it 30 seconds after a cancel, then reports that the last batch may or
  may not have been stored. The line keeps the outcome until the next send. Ctrl+1, 2, and 3
  (Cmd on macOS) switch sections.
- "Sent" counts events the service accepted or did not refuse. The SDK's warning that some
  events were "not durably accepted" goes to the log only. A retried batch keeps its message
  ids, so a lost acknowledgement can show as a duplicate in `eh read`. Sent events are not
  written to the archive; Read captures them like any other event.
- Each send opens its own connection and closes it at the end. A failed close shows as a warning
  under the outcome.
- Rights: an Entra ID identity with the Azure Event Hubs Data Sender role, or a SAS rule with the
  Send right on the event hub. A Listen-only rule fails at the first batch. The `eh` readme has
  the full rights, size, and throttling notes.
- A typed connection string that was never saved is wiped once Connect starts a session, so
  sending while capturing needs a saved secret or Entra ID; otherwise the Send section asks you
  to set up the connection first.

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
- The data folder can change only while no session, bulk export, or send runs. A new data folder
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

Closing the window during a send cancels it; the app quits once the batch in flight and the
connection close end, within about 30 seconds. A send that failed brings the window back.

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
are real), `send` (the Send section with a JSON body, idle), `send-paced` (10 events one a
second, already sending), `send-template` (Template on with an order template and its sample,
idle). Demo sends reach no event hub: they count up following the pacing and
Cancel. The seeded Read states run no engine, so Disconnect there never finishes. Demo mode
reads and writes only under its temporary folder: bulk exports go to its `data/exports`,
whatever folder is picked. Debug builds accept Rune's agent flags
(`--snap`, `--script`, `--tree`, `--trace`, `--scene`); write their output under
`target/eh-gui-demo/`, which git ignores.

```
EH_GUI_DEMO=burst cargo run -p eh-gui --features demo --bin eh-gui-demo
EH_GUI_DEMO=seeded-failed cargo run -p eh-gui --features demo --bin eh-gui-demo -- --snap target/eh-gui-demo/failed.png
```

Release builds of the workspace leave the demo out.

## Headless tests

The view tests, journeys, accessibility and layout sweeps, and seeded input sequences run the
real view in Rune's headless runtime (no window, GPU, or network) with recording services, under
`cargo test -p eh-gui`. Frames, ticks, and deliveries run on a test clock, so a slow machine runs
the same steps; only copy and export feedback stamps the real time, which changes a label, never
what a step presses.
A failing sequence prints `seed=<s> step=<i>` and its last ten actions; it replays from the seed.
One more binary checks that typed secrets reach no log, toast, `Debug` output, or
accessibility tree; it needs the dev-only `test-harness` feature:

```
cargo test -p eh-gui --features demo,test-harness
```

## Visual checks (per release)

Pixel output differs across GPUs and font rasterizers, so screenshots are checked by eye, not in
CI. Snapshot every demo scenario at the smallest window and at the default size, and once at
scale 1.5 for truncated text (a snapshot renders at scale 2 unless `--scale` says otherwise):

```
EH_GUI_DEMO=send cargo run -p eh-gui --features demo --bin eh-gui-demo -- --size 960x600 --snap target/eh-gui-demo/send-960.png
EH_GUI_DEMO=send cargo run -p eh-gui --features demo --bin eh-gui-demo -- --snap target/eh-gui-demo/send.png
EH_GUI_DEMO=send cargo run -p eh-gui --features demo --bin eh-gui-demo -- --scale 1.5 --snap target/eh-gui-demo/send-1.5x.png
```

Snapshots are dark: the app reads the system appearance itself, so `--theme light` changes
nothing. With the system set to light, check in the real window: the sheet and popover scrims,
the pane fills, the shadow and rim on the sheet, popovers, new-messages pill, and bulk bar, the
frosted pill and bar, the Template switch's label, danger and warning text in the send action
row, and the list's selection highlight. A failed send has no demo scenario; check it on a live
hub.

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
- Send section on a live hub: send one event while Read captures the same hub and see it
  arrive; send 1,000 and Cancel midway; send with a partition key and see one partition in
  Read's partition column; send 10 Paced at 1,000 ms (about one a second in Read) and Cancel
  during a wait (stops within a second); close the window during a long send (the app quits
  after the batch in flight).
- Template: turn it on with an order template, Regenerate a few samples, send 5 and see five
  different values in Read; type a broken placeholder and press Send (the editor and caption
  mark it); type into a large template with `RUNE_COMMIT_BUDGET_MS=8` (no commit over budget).
