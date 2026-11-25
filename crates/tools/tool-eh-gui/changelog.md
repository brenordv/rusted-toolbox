# Changelog

## 1.1.0
- Log files go to `~/.rusted-toolbox/eh-gui/logs` (they went to `~/.eh-gui/logs`). The old
  folder is not moved; delete it once you no longer need its logs.

## 1.0.5
- `mod+1`, `mod+2`, and `mod+3` pick a section on a fresh press only: holding the chord no
  longer flips sections as the key repeats.
- The window's theme (`common_gui_rune::Theme`), the section switch (`choice_switch`), the Send
  action row (`action_row`), the section chords (`area_chord`), the Return-types-a-line-break
  key (`type_newline`), the control spring pair (`CONTROL_PAIR`), and the logging boot
  (`common-cli`'s `boot_gui`, which also logs a panic with its thread, location, and message)
  come from the shared libraries. The test harness's `type_into`, focus check, and clipboard
  come from `common-gui-rune`'s. The Send action row is 52 pt tall, the shared row's minimum
  (a control plus 12 pt above and below), 4 pt more than before, so the body box is 4 pt
  shorter. Nothing else the app does changed.

## 1.0.4
- No change in what the app does. `design::parts::reveal`, `design::parts::section_header`,
  and `design::tokens::REVEAL_OFFSET` moved to `common-gui-rune` and are re-exported from the
  paths they had.

## 1.0.3
- No change in what the app does. The chord check and the Return-types-a-line-break check its
  key handlers used moved to `common-gui-rune::keys` (`is_chord`, `is_newline`), which `speak`
  and `alert` use too.
- The connection sheet has a muted hint under each sign-in field saying what goes in it, ending
  with an example value where one helps (the connection string's example on its own line, in
  monospace), and a line under Sign-in method and Azure credential saying what the picked
  option does. Text only: no field, check, or saved value changed.

## 1.0.2
- No change in what the app does. The cleaning of the error details text (each line's control
  and bidirectional characters as spaces, the line breaks kept) moved to
  `common-utils::string_utils::display_lines`, which the `alert` tool uses too.

## 1.0.1
- No change in what the app does. The tool-neutral part of the design system (the kits and
  surface ladder, the shared tokens and motion roles, the caption, label, hairline, detail row,
  and editor parts, the system appearance, the window-failure check, and the headless test
  helpers) moved to the new `common-gui-rune` library, and `display_line` and `error_preview`
  moved to `common-utils::string_utils`. eh-gui re-exports them from the paths they had.

## 1.0.0
- Save config into the launch folder refuses a data folder or export folder outside it, or a
  received message path outside it while Message files is on, which the next launch would not
  use, and says which one. With Message files Off such a path is saved as the default. A data
  folder picked with Change inside the launch folder is held as a relative path, so it saves.
- An archive write that failed no longer stays failed until restart. The next Connect closes
  the archive and opens it again when nothing else holds it; while a session, an export, or a
  search still does, Connect fails with "Writing to the archive failed earlier (cause). It
  reopens once the running session, export, or search ends; then Connect again." Search and
  export keep reading the open archive either way.
- Quit and Force quit end the process without waiting for background work that has not stopped
  (an archive search page, an at-end pass Force quit left behind).
- The 30 s wait at window close covers everything the close waits for (a session, a save, a
  message export, a bulk export, a send), where it covered only a session. The window comes
  back on Read with a notice naming what still runs and Force quit; both go away when that work
  ends, and a second close waits another 30 s. A connection failure under the notice toasts.
- Exports with metadata (the inspectors' Export and Export all matches) carry the event's
  content type, correlation id, and properties; the csv header gained three columns, so a
  one-file-per-month csv with the earlier header is not appended to.
- Disconnect while a session is still connecting ends it at once instead of waiting out the
  SDK's connect retries (about a minute), so the data folder and the profile picker unlock right
  away.
- A session that ignores the checkpoint moves it forward only when it read everything after it.
  One whose time window starts past the checkpoint stores its events and leaves the checkpoint
  where it was, with one warning per partition in the log; the next session reads the events in
  between.
- A launch folder's `config.json` that names a data folder, an export folder, or a received
  message path outside the launch folder (absolute, climbing with `..`, or a network share) no
  longer gets that folder used: the default takes its place before the form is filled, an
  outside received message path also turns Message files off, and the connect sheet's Profile
  group says which settings were not used. A profile's `config.json` keeps an absolute folder.
- Save as profile… at the end of the connect sheet's Saves to row saves the form as a new
  profile in the workspace and opens it: a name field with `eh`'s name rules, a stored secret
  copied into the new profile's secrets file (the row says from which local store), names
  recalled from the launch folder marked and confirmed with Save anyway (Return never confirms),
  the data folder kept inside the profile folder. Both saves into a profile (Save as profile, and
  Save config while a profile is open) fill a blank namespace or event hub from a connection
  string typed in this window, as `eh --save-config --profile` does.
- Profiles: a Profile picker at the top of the connect sheet lists Launch folder and every
  profile in the workspace `eh` uses (`EH_WORKSPACE`, else `~/.eh/profiles`) whose `config.json`
  loads, with a caption counting the ones that can't. A pick reads the profile off the UI thread
  and swaps the form, capture rules, export options, and data folder; Save config writes to the
  picked profile. The picker is disabled while anything uses the data folder, a connect cancelled
  while preparing included, which now holds the folder until it stops. The pill's popover names
  the profile. Demo scenarios `profiles`, `profiles-locked`, `profiles-failed`, and
  `profiles-unavailable`.
- Every switch label (Metadata in both inspectors, the four Export options, Template) is drawn in
  the readable form-label style, and a press on the label or beside the switch toggles it. The
  connection and Received-per-partition popovers fit the kit's 320 pt surface: their paths and
  errors wrap inside it, and a long body (many partitions, a long session log) scrolls inside
  the window. A partition's line reads `12,345,678 · last 18:31:02 UTC`. Demo scenario
  `seeded-partitions` (32 partitions, eight-digit counts, long partition errors); demo scripts
  for the snapshots in `demo-scripts/`.
- The data folder cannot change while a message export (Export… in the Read or Export
  inspector) writes; the connect sheet disables Change and says why.
- The Send sidebar has Correlation id and Properties fields after Pacing. Properties take one
  `key=value` per line, the syntax of `eh send --property`, in a monospace box four lines tall
  that breaks long values anywhere and scrolls inside once it holds text; a bad or repeated line
  is an issue naming it. Both go on every event of the send and are never saved. The `send` demo
  scenario seeds both; demo scripts for the snapshots in `demo-scripts/`.
- A Send press with an issue moves focus to the first field with one and scrolls the sidebar to
  it and its issue.
- Toasts are a stack at the bottom right: the newest card in front, up to two older ones
  peeking above it. The pointer over the stack or Alt+T fans it out, Escape or the pointer
  leaving folds it; info toasts wait while it is fanned out. At most three toasts; the same text
  again rings the front card instead of adding one. View details sits in a card's title row.
  While the details of a toast are open, new toasts wait and come in when they close. Demo
  scenarios `toast-stack` and `toast-stack-open`.
- The content between the sidebar and an open inspector keeps at least 376 pt: a narrow window
  shows the inspector, then the sidebar, narrower, and they come back to their widths when it
  grows; a drag on a pane's edge starts from the width the pane shows. At 960x600 with the
  sidebar open the inspector shows at 320 and its time line wraps. Demo scripts for the
  snapshots in `demo-scripts/`.
- Failure text in toasts, empty states, the connection popover, the Send action row, the bulk
  bar, and the connect sheet shows at most 140 characters; View details opens the whole text in
  a scrolling window with Copy error and Done (Escape closes it). A connection or bulk export
  failure no longer also toasts when its own section is the one in the window. The connection
  pill takes keyboard focus. Demo scenario `error-details`.
- Return in the connect sheet presses its Connect (or Done when it picks where to send).
- Headless reliability tests: journeys through Read, Export, and Send, accessibility and layout
  sweeps over every section at 960x600 and 1280x800, seeded input sequences that replay from
  their seed, and a secret-hygiene test behind the dev-only `test-harness` feature.
- The Send section: a monospace body editor, content type, count, routing (automatic or a
  partition key), and pacing (batched, or one event every 1 to 3,600,000 ms, start to start),
  checked through core's own constructors. The destination is named from the connection sheet
  the way the open resolves it; Edit… opens the sheet with a Done footer that never connects. A
  fixed action row shows the run (connecting, the sent count, the pacing, the elapsed time),
  Cancel (also during a paced wait), and the outcome. Ctrl/Cmd+Enter sends from any Send field;
  Ctrl/Cmd+1..3 pick a section. One producer per send, closed at the end; a send that does not
  stop within 30 s of a cancel is reported. Closing the window cancels a send and waits for it;
  a failed one reopens the window. Demo scenarios `send` and `send-paced`.
- A Template switch on the Send body: `{{type option=value}}` placeholders get fresh mock values
  in every event (mock-data-utils). A placeholder list beside the switch, a fixed six-line
  Sample block with Regenerate, the compile error's line and column in the sample before any
  press and as a field issue after one. Demo scenario `send-template`. `{{now}}` and
  `{{utc-now}}` put the time each event is built into it; the list shows them as the clock
  group. `escape=json` on a placeholder escapes its value for a JSON string; the options list
  shows it. `{{sequence}}` counts the events that show it from 0 while eh-gui runs (the list
  shows it as the counter group, and the Sample shows the next number), and `{{random.guid}}`
  puts in a random GUID.
- Every opening of the secrets file goes through one gate, so Save config, Connect, and Send
  never collide on its lock (Connect during a save is no longer refused).
- Initial release (in progress): the window shell on Rune. A toolbar with the Read and Export
  switch, the connection pill (data folder, log file, version, and the last error in its
  popover), and the connect sheet with the three sign-in methods (connection string, Azure
  credential, Key Vault), config.json recall, Save config, and the Storage settings (data folder,
  message files). Connect opens the data folder's archive, resolves the target as `eh read`
  does, and runs a capture session whose messages go to the archive.
- The Export section, with or without a connection: archive search by time window and text or
  regex in the sidebar (the same fields as the capture rules); results in the same list and
  inspector as the Read section, loaded 250 at a time as you scroll, up to 5,000 rows or 256 MB
  of bodies, with each message once even when it moves between pages; a footer for loading, a
  failed page (Retry), and the cap; empty states for not searched, searching, no matches, and an
  archive that cannot be opened; and a floating bar that exports every match of the last search
  (format, metadata, one file per month, local time, skip already exported, destination folder)
  with live counts every 250 ms, Cancel between messages, and the result with the first failure.
  One bulk export at a time; the data folder cannot change while one runs; closing the window
  cancels it, and a failed one brings the window back. The Read and Export sections share one
  list, inspector, and rules-field implementation.
- Demo scenario `seeded-export`: an archive of 6,000 generated messages and the Export section
  searched for everything. Demo searches and bulk exports stay inside the temporary folder.
- The Read section: a stats strip refreshed once a second (rolling digits; per-partition
  counts behind Received; a UI-dropped tile; quiet-hub and "Not receiving updates" states), a
  virtualized list of the newest 5,000 messages (and at most 256 MB of bodies) that follows the
  tail, lands new rows up to four times a second, and offers a "↓ N new" pill once scrolled up;
  keyboard selection and copy; a quick filter by text or regex over bodies and ids, rebuilt off
  the UI thread; an inspector with metadata, a capped selectable body, Copy body, Copy as JSON,
  export of one message to a chosen file, and a close button; and a session log in the connection pill.
- Capture rules in the sidebar (time window and keywords or regex) apply to the running session
  and its message files, or to the next Connect, and are saved with Save config and recalled
  from `config.json`.
- macOS draws the toolbar as the titlebar (traffic lights over it); Windows and Linux keep the
  system title bar.
- The connect sheet sits under the toolbar and grows with the window; its fields scroll between
  a fixed title and a fixed button row, so Connect is always on screen.
- On Windows the colors follow the system's light or dark setting while the app runs, read once
  a second through `UISettings`.
- The window can't be made smaller than 960x600, so the toolbar never overlaps itself.
- Closing the window during a session stops it and lets the at-end pass finish in the
  background; a failure reopens the window with the error, and a pass still running after 30 s
  reopens it with a Force quit. The archive is closed at quit.
- A data folder change is accepted only while no session runs; the old folder's archive is
  closed right away.
- Logs to a file by default, at info. A window that cannot be created (no usable GPU adapter) is
  logged and exits with a failure instead of leaving a windowless process.
- Typed secrets stay in hidden fields that are wiped after Connect and Save, never appear in the
  element tree or to assistive technology, and a stored secret's bytes never enter the form.
- Design system in `src/design/`: tokens, kit overrides, motion roles, and a contrast test over
  every listed pair in both appearances. The test moved the `Submit` fill to `#0050B8` with a
  darkening hover and press, raised the dark floating tint to 0.88, and draws secondary text on
  selected rows in the primary text color.
- Icons from Lucide 1.73.0 (ISC; license in `assets/icons/LICENSE`).
- Dev-only demo mode (`--features demo`, binary `eh-gui-demo`): a generated event hub and a
  temporary data folder, scenarios chosen with `EH_GUI_DEMO` (including `seeded-read`,
  `seeded-search`, and `seeded-export`), and Rune's agent flags in debug builds. Release builds leave it out.
- Built on Rune (pre-release, not on crates.io), pinned to rev
  `6cf05a0ee0f445c25a33ceeede066b78bd73ef6a` of a fork, `brenordv/rune` (branch `c4eda05-fixes`,
  tag `c4eda05-fixes-2`): anomalyco/rune's rev `c4eda05889c984fff91ccd906e3c00cdfc4cae24` plus
  three commits and nothing else. On Windows a dead key's character types, so quotes type on
  US-International (`'` then Space, `'` twice); a blank line in a multi-line field is one
  accessible text run holding its line break, so a UI Automation read of the Send body's blank
  lines no longer aborts the app; typing, deleting, or pasting in a plain field clears
  `TextEdit::is_pristine`, which upstream's plain-text fast path leaves set. The c4eda05 review
  below covers the rest of that tree, the vendored tiny-skia-path included, which now comes
  from the fork unchanged. Rune's
  `[patch.crates-io]` forks stay on anomalyco/rune at c4eda05 and apply to the whole workspace:
  - winit 0.30.13: Windows window creation without a taskbar COM call and at the requested
    position; a macOS IME reconversion API; Wayland touchpad scroll deltas scaled by 12.
  - wgpu-hal 30.0.1: DX12 adapter exposure, the shader compiler (FXC) loaded on first compile,
    partial presents on a `FLIP_SEQUENTIAL` swap chain.
  - parley_engine 0.12.0: bidi rule L1, shaping caches, segmenter hooks.
  - accesskit_windows 0.35.2: heading and landmark properties for UI Automation.
  - tiny-skia-path 0.11.4 (Rune's own path dependency, not a workspace patch): Skia's conic
    stroking.

  Each fork was checked to be byte-identical to its crates.io release plus Rune's committed
  `vendor/<name>.patch`, and each patch and Rune build script was read by hand: no network,
  file, process, or telemetry code. The `.wasm` that `rune-highlights` translates at build time
  matches the npm `@pierre/highlights` 0.2.0 release. Rune's `tooling` and `keys-overlay`
  features stay off.
