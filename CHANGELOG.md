# Workspace Changelog

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
  `head`, `tail`, and `rxget` use it.
- Reworked the release workflow: each platform publishes one zip per tool
  (`rusted-toolbox_<platform>_<tool>-<version>.zip`) plus an `all` bundle, the Unix bundles skip
  the ported tools, builds run `--locked`, a single publish job uploads everything, and every
  action is pinned to a commit SHA. The release now runs the test suite before packaging, and
  `rust-toolchain.toml` pins Rust 1.98 instead of floating on stable.
- Doc-currency audit across every active crate: 11 readmes corrected against the actual flags and
  output (a `-L` short flag pingx never had, whisper's log-level examples, timestamp's header
  block, and others).
- `cargo test --workspace` now runs 1132 tests.
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