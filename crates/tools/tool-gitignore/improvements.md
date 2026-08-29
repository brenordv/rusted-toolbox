# Basic
[X] Migrate cli to the new pattern.
[X] Research improvements to the app.
[ ] Add more test coverage to the tool.

# Improvements: fold AI ignore handling into gitignore

## Why

The standalone aiignore tool is being retired. It downloads one static template and writes the
same content to eight AI ignore files, whether or not the project uses those tools. Two of its
targets are wrong on their own terms: `.windsurfrules` is an instructions file, not an ignore
file, and `.claudeignore` is a proposal nothing implements yet. Its merge step also unions every
existing AI ignore file into one set before writing, so any per-tool hand-tuning gets flattened
on the next run.

gitignore already does the whole detect, download, merge, sanitize cycle for `.gitignore`. The
plan is to teach it about AI tools instead of keeping a second, weaker copy of that cycle alive.

## Planned behavior

### New `.gitignore` sources for AI tool artifacts

Add mappings in `src/config.rs` that detect AI tool footprints (`.claude`, `.cursor`,
`.windsurf`, `.aider.chat.history.md`, and similar) and queue a curated `ai-tools.gitignore`
template hosted in the [gitignore-files repo](https://github.com/brenordv/gitignore-files).
The goal of that template is keeping local AI state out of git: chat histories, local settings
such as `.claude/settings.local.json`, tool caches. Curate the exact entries against each tool's
docs when building the template; candidates above are unverified until then.

Detection only fires when a footprint already exists on disk. A fresh checkout where the user
runs Claude or Cursor without either having created a folder yet matches nothing, which is why
the flag below exists.

### A repeatable `--ai` flag

```bash
gitignore --ai claude --ai cursor
gitignore --ai claude,cursor
```

clap 4 wiring: `ArgAction::Append`, `value_delimiter(',')`, and a `value_parser` over a provider
enum so unknown names fail with the valid list. Short flags in clap 4 are single characters, so
there is no `-a`; `--ai` only.

Each named provider does two things:

1. Queues the AI artifacts template for the `.gitignore` download set, even with no footprint
   on disk.
2. Creates or updates that provider's own ignore files, listed below.

Merging stays per file: each target file unions with its existing content only, then goes
through the same sanitize path `.gitignore` uses. No cross-file union, so hand-tuned
differences between tools survive.

### Provider map

| `--ai` value | Files maintained                         |
|--------------|------------------------------------------|
| `cursor`     | `.cursorignore`, `.cursorindexingignore` |
| `windsurf`   | `.codeiumignore`                         |
| `gemini`     | `.aiexclude`, `.geminiignore`            |
| `jetbrains`  | `.aiignore`                              |
| `aider`      | `.aiderignore`                           |
| `claude`     | none today                               |

Cursor splits the job in two: `.cursorignore` blocks AI access outright, while
`.cursorindexingignore` only keeps files out of the search index. Both use gitignore syntax.
Cursor also honors `.gitignore` for indexing on its own, so the provider file only needs the
tracked-but-sensitive cases.

Gemini gets two files because the IDE product and the CLI read different ones: Gemini Code
Assist reads `.aiexclude`, the Gemini CLI reads `.geminiignore` (and `.gitignore`).

JetBrains AI Assistant reads `.aiignore` (so does Junie), and falls back to `.cursorignore`,
`.codeiumignore`, or `.aiexclude` when those sit in the project root. So `--ai jetbrains` is
partly redundant if another provider's file already exists; still worth having for
JetBrains-only setups. Note the file must also be enabled in the IDE settings.

Claude Code has no ignore file. Its mechanism is `permissions.deny` rules like `Read(./.env)`
in `.claude/settings.json`, which is a JSON merge this tool should not attempt. `--ai claude`
only contributes the artifact entries to `.gitignore`. Revisit if the upstream `.claudeignore`
feature request lands.

GitHub Copilot stays out of scope entirely: content exclusion is configured in repository or
organization settings on github.com (Business/Enterprise plans), not in a file in the repo.

### Syntax quirks to handle

`.aiexclude` does not support `!` negation and matches `*` greedily across files and
directories. Strip negation lines when writing that file.

Related: the current sanitize step sorts alphabetically, which moves `!` re-include lines ahead
of the patterns they negate. gitignore semantics are last-match-wins, so sorting can change
meaning. The existing `.gitignore` path has the same latent issue; worth fixing while in here,
or at minimum dropping negations from generated AI files.

### Retire tool-aiignore

Remove `crates/tools/tool-aiignore` and its workspace member entry, and note the removal plus
the new flag in the root changelog and readme.

## Implementation sketch

1. `src/models.rs` and `src/cli_utils.rs`: provider enum, flag wiring.
2. `src/config.rs`: provider -> (target files, template URL) table plus the new artifact
   detection mappings.
3. `src/gitignore_app.rs`: provider loop after the `.gitignore` work, reusing the existing
   fetch and sanitize helpers; add per-file merge (no cross-file union) and `.aiexclude`
   negation stripping.
4. gitignore-files repo: add `ai-tools.gitignore` and per-provider templates.
5. Tests: flag parsing, per-file merge isolation, negation stripping, provider-with-no-footprint
   still queues the artifacts template.
6. Delete `crates/tools/tool-aiignore`.

## Sources

Official docs, all verified 2026-08-15:

- Cursor ignore files: <https://cursor.com/docs/reference/ignore-file>
- Windsurf ignore: <https://docs.windsurf.com/context-awareness/windsurf-ignore>
- Gemini Code Assist `.aiexclude`: <https://developers.google.com/gemini-code-assist/docs/create-aiexclude-file>
- Gemini CLI `.geminiignore`: <https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/gemini-ignore.md>
- JetBrains AI Assistant `.aiignore`: <https://www.jetbrains.com/help/ai-assistant/disable-ai-assistant.html>
- JetBrains Junie `.aiignore` (localized mirror; check the .com equivalent): <https://www.jetbrains.com.cn/en-us/help/junie/aiignore.html>
- Aider `--aiderignore` option: <https://aider.chat/docs/config/options.html>
- Claude Code permissions (`permissions.deny`, `Read(...)` rules): <https://code.claude.com/docs/en/permissions>
- `.claudeignore` feature request: <https://github.com/anthropics/claude-code/issues/29455>
- GitHub Copilot content exclusion: <https://docs.github.com/en/copilot/concepts/context/content-exclusion>
- GitHub gitignore templates: <https://github.com/github/gitignore>
- Current template host: <https://github.com/brenordv/gitignore-files>