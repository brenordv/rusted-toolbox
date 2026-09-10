# Changelog

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