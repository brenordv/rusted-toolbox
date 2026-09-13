# Contributing
## Philosophy
Each tool follows a few principles I try to stick to:
- **Do one thing well**: Each tool has a focused purpose
- **Reasonable defaults**: Should work out of the box for common cases
- **Graceful handling**: Proper error messages and cleanup
- **Performance matters**: Use async I/O and parallel processing where it makes sense

## Project structure
The repository is a Cargo workspace. Crates are grouped by role under `crates/`:

1. **Tool crates: `crates/tools/*`**
   - Each CLI tool has its own crate (e.g., `crates/tools/tool-cat`, `crates/tools/tool-jwt`, `crates/tools/tool-csv-split`).
   - Each tool crate contains:
     - `main.rs` (thin entrypoint that orchestrates the tool logic);
     - `cli_utils.rs` (argument parsing/validation and the tool's header printer);
     - `models.rs` (structs and data models);
     - `<tool_name>_app.rs` (the actual tool logic);
     - `readme.md` (manual for the tool);
     - `changelog.md` (per-tool changelog);
     - `improvements.md` (running list of planned and deferred improvements, where one exists; tools with nothing recorded don't carry an empty file);
     - `Cargo.toml` (tool-specific metadata; dependencies are inherited from the workspace, see below);
     - Additional files as needed to keep things tidy, scoped to the tool.
2. **Library crates: `crates/libs/*`**
   - `crates/libs/common-utils`: general-purpose helpers (datetime, strings, file system, shared constants), kept as dependency-free as possible.
   - `crates/libs/common-utils-ext`: helpers that pull in heavier dependencies and that only some tools need (clipboard, GUID generation, regex sanitizing).
   - `crates/libs/common-cli`: the common CLI arg parser, header formatting, exit helpers, and basic logging used by all tools.
   - `crates/libs/common-file-utils`: file-system operations more specialized than the ones in `common-utils`.
   - `crates/libs/common-serialization-utils`: loads serialized data (JSON) from files into typed objects.
   - `crates/libs/cli-signal-monitor`: watches for Ctrl+C and runs the tool's graceful-shutdown path.
   - `crates/libs/logging-otel`: OpenTelemetry extension for the logging in `common-cli`.
   - `crates/libs/mock-data-utils`: the mock-data generators behind `tool-mock`.
   - `crates/libs/shared-eventhub`: EventHub-specific shared code.
   - `crates/libs/shared-head-tail`: the shared engine machinery behind `tool-head` and `tool-tail`.
   - If more than one tool needs it, it belongs in one of these crates rather than duplicating logic.
3. **Root `README.md`**: add a reference to any new tool here, and keep the links pointing at `crates/tools/<crate>/readme.md`.
4. **Build scripts (`build.sh`, `build.bat`)**: they build the whole workspace in one pass and copy every produced binary into `dist/` (`dist\windows\` on Windows). There is no per-tool list to maintain: a new tool crate is picked up automatically. On non-Windows systems, `cat`, `touch`, `head`, and `tail` are built but not copied, since coreutils already provides them.

## Dependencies (centralized)
Dependency versions live in one place: the root `Cargo.toml`.

- **Versions**: declared once under `[workspace.dependencies]`. A crate uses one by referencing it as `dep = { workspace = true }` in its own `[dependencies]`. To add a new external crate, add it to the root table first, then reference it from the tool.
- **Per-crate features** are additive: `clap = { workspace = true, features = ["color"] }` adds `color` on top of the workspace default.
- **Optional deps**: the workspace table cannot mark a dep `optional`, but the member can: `raccoon-otel = { workspace = true, optional = true }`.
- **Shared package metadata** (`edition`, `authors`, `repository`, `license`) lives under `[workspace.package]`; each crate opts in with `edition.workspace = true` and the like. Keep `name`, `version`, and `description` per-crate.
- **Bumping a version** is a one-line edit in the root table. Keep versions current with a periodic `cargo update` / `cargo upgrade`.

## Adding a new tool
1. Create `crates/tools/tool-<name>` with the file layout above. The `crates/tools/*` glob in the root `Cargo.toml` picks it up; no `members` edit is needed.
2. Inherit metadata (`edition.workspace = true`, etc.) and dependencies (`common-cli = { workspace = true }`, ...) from the workspace.
3. Add the tool to `README.md`.

## Continuous integration
- `.github/workflows/ci.yml` runs on every push to `master` and every PR: `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --locked -- -D warnings` on Linux, plus `cargo test --workspace --locked` on Windows, Linux, and macOS.
- `.github/workflows/deps-audit.yml` runs `cargo deny check advisories` (RustSec database, config in `deny.toml`) weekly and whenever a PR or push touches a manifest, `Cargo.lock`, `deny.toml`, or the workflow itself.
- `.github/dependabot.yml` keeps the SHA-pinned actions in the workflows fresh with weekly PRs.
- `.github/workflows/release.yml` runs on `v*` tags: it builds release binaries on each OS and attaches archives to a GitHub Release.
- `rust-toolchain.toml` pins the toolchain to a specific version (currently 1.98, bumped deliberately alongside a clippy/fmt pass) and ensures `rustfmt` and `clippy` are available.

## Tool structure
1. Tools can print/log information, trace, and warnings, but not errors;
2. If any errors that need to stop the execution happen, return the error to the entrypoint and use `context` (from Anyhow) to add info on what went wrong;
3. If the execution is successful, use `exit_success()`;
4. If the execution fails, use `exit_error()` to exit the app (from the entrypoint file only). When a tool needs specific exit codes, use `exit_with_code()` (all three helpers live in `common-cli`) and document the codes in that tool's readme;
5. The runtime-info header is opt-in: every tool exposes the shared `--app-header` flag, and `common-cli`'s boot path prints the standard header (plus the tool's own config section, when it registers one) only when the flag is passed.

## Coding principles
- Favor small, well-factored modules and explicit types over cleverness.
- Respect existing patterns; follow the repo's conventions over personal preference.
- Functions around 50 LOC when feasible; extract pure helpers for parsing, graph building, and process execution.
- Never use nested ternaries (Rust's `if/else if/else` or match statements keep control flow clear).
- Avoid `unwrap`/`expect` in library code; return typed errors. Use `?` (from `anyhow`) for propagation and convert to a single error type at the boundary.
- Use the repo's `edition` from `Cargo.toml`; don't change it without approval.
- When defining the CLI options, follow the examples of the other tools.
- Use the default `rustfmt` and `clippy` (there is no custom config); fix all warnings.
- Forbid `unsafe_code` unless an explicit, justified exception is approved.
- Before creating a new tool/utility, check if it is already covered by one of the `crates/libs` crates.
- When planning tests and usages, consider edge cases, but within reason.

## Other
1. YAGNI: Let's try to keep the code simple, adding parallelism and more complex features as the need arises.
2. Be nice.
