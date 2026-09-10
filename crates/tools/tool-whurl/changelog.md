# Changelog

## 3.1.0
- Elapsed-time display (issue 38): each entry logs `[Elapsed: 121 ms | Total: 147 ms]` after its calls, and a repeat hit of the same method + URL (+ effective environment) within one run adds a signed delta, e.g. `[Elapsed: 25 ms (-96 ms) | Total: 147 ms]`. The `--test` summary gained a per-entry elapsed column. Durations print as whole milliseconds under one second and one-decimal seconds above; totals include entries hidden by `silent` includes; the summary column prints whenever the summary prints. The JSON report is unchanged.
- Per-include environment override (issue 39): `# @include:[env=NAME] path` runs the included file's API against `NAME` instead of the run's `--env`, inherits down that include's subtree (an include's own `env=` beats an inherited one), and applies even when no `--env` was passed. Environments resolve per API; conflicting overrides for one API keep the first and warn. Applying an override logs an info event citing the directive, and a missing environment now cites the directive that selected it. Unknown `key=value` include options log a warning instead of being silently ignored.
- Inline variable feed (issue 41): `# @include path -> { key="value", n=42, ref={{name}} }` hands variables to the run when that include is pulled in. Feeds apply after every file-based layer and lose only to `--vars-file`/`--var`; a `{{name}}` reference resolves to the variable's final value for CLI-supplied names and fails before execution when unknown, citing file and line. Every occurrence's feed applies in order (last writer wins per key). Malformed feed clauses, trailing text after the closing brace included, are parse errors with file and line; `dry-run` now fails on them too (exit code 2).
- In-run `$shell` cache: a byte-identical `$shell(...)` expression executes once per run; later occurrences reuse the first result and the assignment log marks them `cached=true`. Only successful results are cached, and only for `$shell` (never `$uuid`/`$int`/other generators). Behavior note: a non-deterministic command duplicated across files now yields one shared value per run.
- Log hygiene: the `--var` collision origin names the key only instead of `key=value` (the old form could put values into warning logs, which can reach `--log-to-file`); include-feed origins carry the include and file:line, never values.
- Readme: new "Fetching secrets with $shell" section (per-invocation gate, Azure Key Vault example under a masked `api_token` name, the name-keyed masking rule spelled out, error-output caveats, trust statement for `.dvars` under the gate), include-option and feed documentation, elapsed-time notes, a concrete `env=` include example, a `--test` example, and a pointer to the runnable samples under `requests/httpbin/`.

## 3.0.0
- `run` and `dry-run` now flatten the shared `common-cli` flags: `--app-header`, `--log-level`, `--log-to-console`, `--log-to-file`, and `--rotate-log-file-by-day`. The tool-owned `-v`/`--verbose` count keeps feeding only the embedded Hurl engine's verbosity, and the typed exit codes are unchanged.
- Breaking: the bespoke runtime header no longer prints on every non-silent run. It moved behind the opt-in `--app-header` flag in the standard toolbox format, and its `Request` line shows the FILE argument as passed instead of the resolved path (the header prints before path resolution). Scripts scraping the old header must opt in or stop parsing it. In the silent modes (`--silent`, `--print-only-*`) the header stays suppressed even when `--app-header` is set.
- Log-level precedence: whurl still derives Error under `--silent`/`--print-only-*` and Info otherwise, but an explicit `--log-level` now overrides that derivation. `RUST_LOG` outranks the resolved level when set; only `--log-level disabled` silences everything including `RUST_LOG`.
- Dynamic-variable assignment logs now carry the variable name, source file, and line, without the value: dvars values routinely hold secrets, and the tracing stream can reach a plaintext log file once `--log-to-file` is in play.
- With `--log-to-file`, execution logs are appended under the tool's logs folder in the user's home; they include response bodies, so treat that file accordingly.

## 2.0.1
- Recorded seven design plans in `improvements.md`: consolidate the CLI onto CommonToolArgs, inline variable feed on `@include` (issue 41), stress/load test support (issue 40), per-include environment override (issue 39), elapsed-time display (issue 38), shell-command variables for secrets (documented path), and in-run variable cache (auth tokens).
- Added tests over tempfile trees: `Includer::merge` (children before parents, duplicate includes expand once, boundary markers, line map), `IncludeTracker` (cycle path, self-include, diamond), `FileResolver` traversal guards (`..`, absolute, backslash, NUL, outside-root), `parse_top_comment_directives` options, and `build_variables` precedence (inline `--var` > `--vars-file` > current file `@vars` > `_global`).
- Readme corrections: the runtime example header now shows the current version, the Windows build section and sample output reference hurl 8.0.1 (the tag the workspace builds), and the `--env` lookup order is documented as it actually resolves (`<API>/NAME.hurlvars` first, then `_vars/NAME.hurlvars`, with `.dvars` also accepted).

## 2.0.0
- Refactored to fit the new tooling: CLI parsing now uses the `clap` derive API (with derived subcommands) instead of the hand-built `Command` from the retired `shared` crate.
- Logging and process-exit helpers now come from the shared `common-cli` crate; the tool keeps its own log-level resolution (Info by default, Error under `--silent`/`--print-only-*`) and its typed exit codes.
- No change to the command surface: `list`, `run`, and `dry-run` and all of their flags behave as before.

## 1.3.0
- Allow `# @vars` directives to load matching `.hurlvars` files before `.dvars`, and fail fast when neither exists (plus regression tests).
- Some house cleaning.
- Updated the macOS convenience script to only install pkg-config if it is missing.

## 1.2.1
- Fixed `--env` resolution to accept either `<env>.hurlvars` or `<env>.dvars`, instead of requiring both files to exist.

## 1.2.0
- Added runtime dynamic variables (`.dvars`) with generators for timestamps, random data, and optional shell execution.
- Introduced the `# @vars` directive and automatic loading of `_global.dvars` and `<env>.dvars` across include graphs.
- Hardened `$shell(...)` dynamic values with an allow-list toggle, cross-platform shell selection, and destructive command detection.
- Require at least one `<env>.hurlvars` or `<env>.dvars` file whenever `--env` is used, preventing partially defined environments.
- Renamed `--print-only-result` to `--print-only-full-response` and now pretty-print the JSON report when streamed to stdout.
- Added a `--print-only-response-body` mode to emit just the final response body without headers or logs.
- Ensure a blank line is printed for 204 responses so the terminal output reflects execution.

## 1.1.0
- Added support for per-API `_global.hurlvars` files that always load alongside the selected environment.
- Automatically load env/global variable files for cross-API includes to keep shared requests in sync.
- Emit warnings (when not in silent mode) whenever variable sources collide during merge.
- Documented global variables and clarified `--file-root` usage with practical examples in the README.

## 1.0.0
- Initial release.