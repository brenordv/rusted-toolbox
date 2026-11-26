# Workspace Changelog

## v9
- `eh-gui`'s connection sheet has a muted hint under each sign-in field saying what goes in it,
  with an example value where one helps, and a line under Sign-in method and Azure credential
  saying what the picked option does. Text only: no field, check, or saved value changed.
- `eh`'s connection flags (`read` and `send`) show an example value in their help, and the
  menu's namespace and event hub prompts give one too. Text only.
- GUI revamp, shared layer: `common-gui-rune` 1.2.0 gains `Theme`, the parts `action_row`,
  `choice_switch`, `reveal`, and `section_header`, the keys `area_chord` and `type_newline`,
  `motion::CONTROL_PAIR`, `window::run_watched` and `report_failure`, and more headless test
  helpers; its focus rings and badge labels are in the contrast test. `common-cli` 1.9.0 gains
  `gui_boot::boot_gui`, the logging boot every window uses, which also logs a panic. `speak`
  1.2.1 and `alert` 1.0.2 use them in place of their own copies (speak's `mod+1`/`mod+2` now
  ignore a held key); `eh-gui` 1.0.4 re-exports the two parts and the token that moved.

## v8
- New library: `common-gui-rune` 1.0.0, the tool-neutral part of eh-gui's Rune design system
  (kits and surface ladder, tokens, motion roles, parts, the system appearance, a window watch
  for small apps, and headless test helpers behind its `test-harness` feature), moved out of
  `eh-gui` (now 1.0.1, no behavior change) so other Rune tools share one copy.
  `common-utils` 2.3.0 gains `string_utils::display_line` and `error_preview`, also moved from
  eh-gui.
- New tool: `speak` 1.0.0 (`crates/tools/tool-speak`) sends text, from its argument or a piped
  stdin, to a text-to-speech server and plays the audio it answers with. The server address is
  asked on the first run in a terminal and kept in `~/.rusted-toolbox/tool-speak/config.json`;
  no flag sets it. An agent skill ships beside it (`crates/tools/tool-speak/skill/SKILL.md`).
- New library: `common-audio` 1.0.0 decodes a sound (WAV, MP3, FLAC, Ogg Vorbis) and plays it
  on the default output device. Workspace dependency added: `rodio` 0.23.0 (features
  `playback`, `wav`, `mp3`, `flac`, `vorbis`). On Linux the build now needs the ALSA
  development files (`libasound2-dev`): CI and the release workflow install them, and
  `convenience-build-ubuntu.sh` leaves the tools that play sound out, with a warning, when it
  cannot.
- `common-serialization-utils` 1.2.0 gains `save_object_to_json_file` (pretty JSON, written
  atomically).
- New tool: `alert` 1.0.0 (`crates/tools/tool-alert`) shows a window above other windows with
  a message, an optional title, and an optional sound, and returns when the user closes it.
  `alert --gui` opens a tester for it. An agent skill ships beside it
  (`crates/tools/tool-alert/skill/SKILL.md`). It plays sound through `common-audio`, so
  `convenience-build-ubuntu.sh` leaves it out with `speak` when the ALSA files are missing.
  `common-utils` 2.4.0 gains `string_utils::display_lines`, moved from `eh-gui` (now 1.0.2, no
  behavior change), which cleans the alert's message.
- `speak` 1.1.0 adds `-g`/`--gui`: a window with a Compose area (the text, a button per vocal
  reaction that types its tag at the caret, Generate, Play) and a Settings area that saves the
  server address, method, model, voice, and reaction list. A first run with `--gui` opens on
  Settings. `common-gui-rune` 1.1.0 gains `parts::line_field` (a labelled single-line field
  whose key handler runs before the field's), `appearance::set_appearance`,
  `window::gpu_hint`, and `keys::{is_chord, is_newline}`; `alert` 1.0.1 and `eh-gui` 1.0.3 use
  them in place of their own copies. The one visible change: Shift+Return types a line break
  in the alert tester's Body.
- `speak` 1.2.0: the window's Settings area has a Server field and an Endpoint field where it
  had one URL field. Save joins them into the settings file's one `url`, so the file and the
  command line are unchanged.

## v7
- Rebuilt the Azure Event Hubs tooling on the official `azure_messaging_eventhubs` SDK and a redb
  archive:
  - New tool: `eh` 1.0.0 (`crates/tools/tool-eh`), one CLI with `eh read` (capture into the local
    archive), `eh export` (txt, csv, or json), and `eh send` (one body sent `-n` times, batched
    or one every `--interval-ms`; `--template` placeholders that get fresh mock values in every
    event; `--dry-run` prints the rendered events instead of sending; `--property` and
    `--correlation-id` on every event). Profiles are saved connections in a workspace
    (`.eh/profiles` in the home folder, or the folder in `EH_WORKSPACE`): `--profile NAME` uses
    one and `eh profile list` shows them. `eh` with no arguments in a terminal opens a menu.
  - New tool: `eh-gui` 1.0.0 (`crates/tools/tool-eh-gui`), a desktop app on Rune with a Read
    section (live capture, statistics, quick filter, inspector), an Export section (archive
    search by time window and text or regex, results loaded as you scroll, export of one message
    or of every match with live counts and cancel), and a Send section (body editor, templates,
    pacing, correlation id and properties). The connect sheet has a Profile picker over the
    workspace `eh` uses, and Save as profile saves the form as a new one.
  - New libraries: `shared-azure` (credentials and Key Vault secrets), `shared-eventhub-core`
    (message model, capture filters, the capture engine, statistics), and
    `shared-eventhub-persistence` (redb archive, checkpoints, export log, exporters, config.json
    and the secrets file).
  - Removed: `tool-eventhub-read` (`eh-read`), `tool-eventhub-export` (`eh-export`), and the
    `shared-eventhub` library. Data captured by `eh-read` 1.1.0 (sled) is not migrated; the last
    released 1.1.0 binaries still read it.
- Workspace dependencies added: `azure_core` 1.2.0-beta.2, `azure_identity` 1.1.0-beta.2,
  `azure_messaging_eventhubs` 0.16.0, `azure_security_keyvault_secrets` 1.1.0-beta.2,
  `azure_core_amqp` 1.2.0-beta.3 (test fixtures only), `redb` 5.0.0, `async-trait` 0.1.92
  (`shared-azure`), `inquire` 0.9.5 (`eh`'s menu, crossterm backend only), `rustls` 0.23.46
  (`shared-eventhub-core` sets the process crypto provider), `windows` 0.62.2 (eh-gui
  reads the Windows appearance through `UISettings`), and `rune` and `rune-kit` as git
  dependencies pinned to a fork (`brenordv/rune`, rev `6cf05a0ee0f445c25a33ceeede066b78bd73ef6a`):
  anomalyco/rune's rev `c4eda05889c984fff91ccd906e3c00cdfc4cae24` plus fixes for Windows dead
  keys, blank lines in a field's accessible text, and `TextEdit::is_pristine` after an edit of a
  plain field. Removed: `azeventhubs`, `sled`, and `rand06-compat`.
- Major dependency bumps: `rsa` 0.9.10 to 0.10.0 (`whisper`), `jsonwebtoken` 11.1.0 to 12.0.0
  (`jwt`), `rusqlite` 0.41.1 to 0.43.0 (`netquality`), `rfd` 0.17.2 to 0.18.0 (`seal-gui`),
  `notify` 8.2.2 to 9.0.0 with `notify-debouncer-full` 0.7.0 to 0.8.0 (`seal`'s watch mode), and
  `rstest` 0.28.0 to 0.29.0 (tests).
- `url` 2.5.8 to 2.5.9 for RUSTSEC-2026-0592 (a stack overflow in `Url::origin()` on nested
  `blob:` URLs); `form_urlencoded` moves to 1.2.3 with it.
- Rune's `[patch.crates-io]` block (forks of `winit`, `wgpu-hal`, `parley_engine`, and
  `accesskit_windows` at anomalyco/rune's rev `c4eda05`) is copied into the root `Cargo.toml`
  and applies to the whole workspace, `seal-gui`'s eframe stack included. `wgpu-hal` stays at
  Rune's 30.0.1.

## v6
- Updated dependencies.

## v5
- Normalized CLI configuration for the tools, so they will show help when arguments are required, but were not provided.
- Write via temp file routine was promoted from an `imgx` routine to a shared logic in common-file-utils.
- Updated `imgx` tool to use this new shared routine.

## v4
- Updated Rust from edition 2021 to 2024.

## v3
- Went through every active crate (the 21 tools plus the 9 non-eventhub libs) fixing bugs, adding
  tests, and bringing each readme and changelog in line with what the code actually does. Every
  touched crate got a version bump; the notable ones are `common-file-utils` 1.0.0 -> 2.0.1 and
  `common-cli` -> 1.4.1.
- Notable fixes:
  - `tool-qrcode`: Wi-Fi payloads backslash-escape reserved characters, and the password is
    masked in the header and redacted from debug logs.
  - `tool-http-server`: fixed an XSS hole in the directory listing; entry names are HTML-escaped
    and hrefs percent-encoded.
  - `tool-whisper`: incoming frames are capped at 64 KiB, and send/receive errors show up as
    `[system]` chat lines instead of killing a thread silently.
  - `tool-netquality`: OpenTelemetry export is wired through `logging-otel`; the endpoint value
    is never logged.
  - `tool-jwt`: an `exp` outside the representable timestamp range is reported as invalid instead
    of silently treated as expired just now.
- `whurl` 3.0.0: `run` and `dry-run` flatten the shared `common-cli` flags. Breaking: the runtime
  header is now opt-in via `--app-header`.
- `whurl` 3.1.0: per-entry elapsed-time display with repeat-hit deltas and a `--test` elapsed
  column; `# @include:[env=NAME]` per-include environment overrides; inline variable feeds
  (`# @include path -> { k=v, ref={{name}} }`); an in-run cache so a duplicated `$shell(...)`
  expression executes once per run; the readme documents `$shell` as the secret-fetch path.
  Stress/load support (issue 40) stays a recorded spec, deferred.
- `common-cli` 1.4.0 added the `CommonToolArgsNoVerbose` boot variant (whurl's migration path);
  1.4.1 hardened `--log-to-file` on Unix (logs dir 0o700, file 0o600, pre-existing files tightened
  on open).
- `touch` 2.1.0: `-d` accepts the POSIX `YYYY-MM-DDThh:mm:SS[.frac][Z]` forms and honors offsets,
  the `-t` century pivot follows POSIX (69), and argument errors print instead of exiting silently.
- New tools: `head` and `tail` (both 1.1.0), GNU ports built on the new `shared-head-tail` lib.
  They speak GNU's count grammar (lowercase `k`/`m`, `B`/`iB` modifiers on every scale letter),
  saturate oversized counts instead of erroring, follow files by polling with rotation detection,
  and exit 0/1/130. Like `cat` and `touch`, they stay out of the Unix bundles and convenience
  installs so they never shadow the system binaries.
- New tool: `rxget` 1.0.0 pulls regex-matched values out of text files, with a bytes-level line
  engine, self-contained glob expansion, per-file or per-run uniqueness, and `-H` filename
  prefixes.
- `remove-zw` 2.1.0: `--check` reports findings without rewriting and exits diff-style (0 clean,
  1 findings, 2 error); `--keep-bom` preserves a leading UTF-8 BOM.
- `common-cli` 1.5.0: new `broken_pipe` module so tools exit cleanly when stdout closes early;
  `head`, `tail`, and `rxget` use it. 1.7.0 made the shared `--app-header` block broken-pipe
  safe for the whole fleet and added the `test_writers` module, which replaced the private
  failing-writer test doubles eleven crates carried.
- `csv-split` 3.0.x: the file splitter formerly named `split` (the old name shadowed coreutils
  `split`). A failed part flush exits 1 instead of passing for a success, and every stdout
  print in the tool is broken-pipe safe.
- `https` 2.2.0 through 2.4.2: file responses stream in 64 KiB chunks with HTTP Range support
  (206/416), directories download as streamed zip archives (`?download=zip`, bounded memory,
  at most 4 concurrent builds), HEAD answers with GET's headers, files carry
  `Last-Modified`/`ETag` validators with `If-Range` and conditional-GET 304 handling, a
  symlink alias can no longer expose hidden entries without `--serve-hidden`, and the startup
  prints are broken-pipe safe.
- `https` 2.5.0: the directory listing got its 📁/📄 glyphs back and links zip downloads
  directly: a "download this directory" link on every listing page and a `zip` link on every
  subdirectory row. `?download=zip` stays the scripting form.
- `https` 2.6.0: listings download selections: a checkbox per entry plus a "Download selected as
  .zip" button, backed by repeatable `pick` query parameters
  (`?download=zip&pick=src&pick=readme.md` works from curl too). Picks are direct children only,
  validated all-or-nothing before the response starts, capped at 512, with symlinks not
  selectable and hidden entries still behind `--serve-hidden`.
- `https` 2.7.0: the listing header carries a select-all checkbox that ticks or clears every
  entry at once. It is driven by the page's one static script (nothing is interpolated into
  it), never submits a form field of its own, and stays hidden when JavaScript is off.
- `rxget` 1.1.0: a `-` target reads standard input, mixing with files and patterns.
- `tail` 1.3.0: GNU's obsolete first-argument forms (`tail -5`, `tail +20lf`) are accepted,
  matching `head` 1.2.0's obsolete-form support.
- `netquality` 2.1.4: the Telegram bot token renders as `<redacted>` in every Debug output,
  the docs steer token delivery to the config file, and a failed Telegram send is recorded on
  the `netquality.notification` span.
- `gitignore` 2.1.2 prunes ignored directories during the scan walk; `imgx` 2.1.3 encodes to a
  temp file and renames over the destination so a failed encode cannot truncate an existing
  image.
- Reworked the release workflow: each platform publishes one zip per tool
  (`rusted-toolbox_<platform>_<tool>-<version>.zip`) plus an `all` bundle, the Unix bundles skip
  the ported tools, builds run `--locked`, a single publish job uploads everything, and every
  action is pinned to a commit SHA. The release now runs the test suite before packaging, and
  `rust-toolchain.toml` pins Rust 1.98 instead of floating on stable.
- Doc-currency audit across every active crate: 11 readmes corrected against the actual flags and
  output (a `-L` short flag pingx never had, whisper's log-level examples, timestamp's header
  block, and others).
- `cargo test --workspace` now runs 1444 tests.
- Added an `improvements.md` to every active crate: a running list of planned and deferred
  improvements.
- `gitignore` 2.1.0: new `--ai` flag and AI-agent footprint detection (`.claude`, `.cursor`,
  `.windsurf`, and friends) queue the upstream `Global/Agents.gitignore` artifacts template, so
  local agent state stays out of git; Cursor detection moved off the third-party `oslook`
  template. This absorbs what was left of the retired `tool-aiignore`.
- Updated the root `README.md` (trimmed the tool list to the 23 tools that still exist, removed
  dead demo links) and `CONTRIBUTING.md` (current `crates/libs` layout, real exit-code
  convention), and added this changelog entry.
- The macOS/Ubuntu convenience installers now skip all four ported Unix tools (`cat`, `head`,
  `tail`, `touch`); they only skipped `cat` and `touch`, so a convenience install shadowed the
  system `head` and `tail`. Pruned eleven `[workspace.dependencies]` entries no crate uses
  (`async-trait`, `bytes`, `dialoguer`, `futures`, `log`, `notify`, `proc-macro2`, `quote`,
  `shell-words`, `syn`, `tokio-stream`).
- The macOS convenience installer caught up with the Ubuntu one: fast-forward-only pull (a
  diverged clone now fails loudly instead of merging), clone detection via `.git`, explicit
  git/curl checks with install hints, and per-shell rc-file handling for the PATH update. Also
  dropped a leftover placeholder from its usage comment.
- Left the EventHub crates (`tool-eventhub-read`, `tool-eventhub-export`, `shared-eventhub`)
  untouched; they are on hold.

## v2
- Splitting the code in `shared` crate into smaller, specialized crates, and also removing the crate `shared`, since it is no longer needed.
- Removed the following tools: `ai-tool-chatbot`, `ai-tool-how`, `tool-aiignore`, and `tool-distro-cc`. 
- Removed the `ai-macros`, and `ai-shared` lib crates.

## v1

First release after the major refactor. This version reorganizes the whole
workspace, modernizes the dependency tree, and adds CI/CD.

### Repository structure
- Split the flat `crates/<name>` layout into `crates/libs/*` for the library
  crates (`ai-macros`, `ai-shared`, `shared`, `shared-eventhub`) and
  `crates/tools/*` for the 27 binary tools.
- Rewrote the root workspace manifest and every per-crate `Cargo.toml` to point
  at the new paths.
- Updated `build.sh`, `build.bat`, and the macOS/Ubuntu convenience build
  scripts for the new layout.
- Revised `README.md` and `CONTRIBUTING.md` to match the new structure.

### CI/CD
- Added GitHub Actions workflows for CI (`ci.yml`) and release (`release.yml`).
- Pinned the toolchain with `rust-toolchain.toml`.

### Dependencies
- Bumped the workspace dependency set to current versions, including the major
  upgrades `fake` 4 -> 5, `hurl`/`hurl_core` 7 -> 8, `jsonwebtoken` 9 -> 11,
  `rand` 0.9 -> 0.10, `ratatui` 0.29 -> 0.30, `reqwest` 0.12 -> 0.13, `syn`
  2 -> 3, `rusqlite` 0.38 -> 0.40, `string-interner` 0.19 -> 0.20, and
  `surge-ping` 0.8 -> 0.9, plus many minor and patch bumps across the tree.
- Made `reqwest` 0.13 the workspace default. `tool-netquality` pins `reqwest`
  0.12 to stay compatible with `cfspeedtest`.

### Code updated for the new dependency APIs
- `tool-jwt`: decode via `jsonwebtoken`'s `dangerous::insecure_decode`, dropping
  the manual validation and algorithm-list setup.
- `tool-mock` and `tool-whurl`: moved from `rand::Rng` to `rand::RngExt` for the
  `rand` 0.10 API.
- `tool-whurl`: switched `follow_location` to the `hurl` 8
  `FollowLocation`/`CredentialForwarding` API.