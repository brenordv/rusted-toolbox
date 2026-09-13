# Changelog

## 2.1.2
- The detection walk no longer descends into ignorable folders (`target/`, `node_modules/`,
  `.venv/`, the IDE and AI agent state dirs, ...). The folders themselves still surface as
  footprints, so `.vscode`, `.idea`, and `.claude` keep queueing their templates. Files inside
  them were already excluded from detection and now stop being enumerated at all; footprint dirs
  nested inside an ignorable folder (a `.vscode` under `build/`) no longer surface either, where
  before they still queued their template. Big build trees no longer cost the walk anything.
- The ignorable-ancestor check is bounded at the walk root. Directory components above the target
  folder used to count, so a project living under a directory named `target`, `build`, or `dist`
  got zero detection; the walk root itself is also never pruned, whatever its name.
- Walk entries that cannot be read are recorded at debug level instead of vanishing silently, and
  each pruned folder is visible at debug level too.

## 2.1.1
- `.m` files no longer fetch Matlab rules unconditionally. The extension was registered for both
  Objective-C and Matlab, and the Matlab entry silently won. Detection now records `.m` and picks
  the template after the walk: a `.mm` companion means Objective-C, a `.mat` companion means
  Matlab, and with no companion evidence (or both) both templates are queued. Both URLs were
  already part of the static map, so this adds no new template source.
- Files under `target/` folders no longer feed detection, matching the readme's Smart Filtering
  section. The walk still enumerates those folders; skipping the descent entirely is a separate
  backlog item.
- Template downloads run with a 30-second total per-request timeout. reqwest applies none by
  default, so a hung template host used to stall the run forever. Fetch failures now name the
  failing URL in the error chain.
- Run errors are logged through the standard `error!` arm in main instead of anyhow's `Debug`
  print, matching the sibling tools. `--log-level disabled` now silences their text (the exit
  code stays 1); the pre-boot target-folder validation error prints directly to stderr.
- Internal: the extension map is a `BTreeMap`, which drops the separately maintained sorted key
  list and a production `unwrap`.

## 2.1.0
- New `--ai` flag: queues the AI agent artifacts template for the `.gitignore` merge even when no
  agent footprint exists in the target folder.
- AI agent footprints now queue that template automatically: the `.claude`, `.cursor`, `.windsurf`,
  `.gemini`, `.continue`, `.cline`, `.codex`, and `.codeium` state dirs, plus `.cursorrules`,
  `CLAUDE.local.md`, the Aider history files, and the Gemini CLI debug log and clipboard dir.
- The template is upstream `github/gitignore` `Global/Agents.gitignore`, taken as-is. Its active
  entries cover local state for Aider, Claude Code, and Gemini CLI; the other agents it lists are
  commented-out examples, and the merge drops comments, so those contribute nothing until the
  template URL is swapped for a fork that opts them in.
- `.cursor` and `.cursorrules` detections now queue that template instead of the
  `oslook/cursor-ai-downloads` repo's own `.gitignore`, which was never a curated Cursor template.
  The `.cursor-tmp` and `cursor-output` detection keys are removed; they mirrored that repo's
  layout, not documented Cursor state.
- Files inside the AI agent state dirs listed above no longer feed detection (a
  `.claude/hooks/foo.py` no longer queues the Python template), matching how `.vscode` and `.idea`
  contents are handled. The dirs themselves still count as footprints.

## 2.0.1
- Merged `.gitignore` files are no longer sorted alphabetically. The existing file's lines keep
  their original order and come first, downloaded template lines follow in fetch order, and
  duplicates collapse to the first occurrence. The old sort could hoist `!` re-include lines above
  the patterns they negate, silently inverting their meaning (gitignore is last-match-wins); that
  ordering is now preserved.
- Download order is deterministic: pending template URLs are collected in sorted order instead of
  hash order, so repeated runs produce the same file.
- The directory walk no longer panics on a non-UTF-8 path; such paths are skipped with a warning.
- The `--app-header` tool section now renders through `common-cli`'s `header_format` helpers.
  Output bytes are unchanged.
- Replaced the test that pinned sorted output with order-preservation, negation-ordering, and
  sanitize tests, and added detection tests for compound and multi-extension filename keys.
- Readme: the header example now matches the real `--app-header` output (the header does not print
  on every run), and the merge behavior description reflects the order-preserving merge.

## 2.0.0
- Refactored to fit the new tooling.

## 1.1.0
- Added `.slnx` to the watched list of files for C#.
- Updated dependencies.

## 1.0.4
- Updated dependencies.

## 1.0.3
- Removed emojis. They don't render properly on every terminal.

## 1.0.2
- Added support for `Sqlite` files.

## 1.0.1
- Improved logic that ignores files, so we're not ignoring IDE-specific folders anymore.
- Also added to that logic the `.venv`, and `__pycache__` folders.

## 1.0.0
Initial release