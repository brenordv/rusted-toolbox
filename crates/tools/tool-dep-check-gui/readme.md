# dep-check-gui

A window for [dep-check](../tool-dep-check/readme.md): pick an ecosystem, type or paste the
packages, press Check, and read what OSV.dev, deps.dev, and the GitHub Advisory Database know
about each one. It runs the same engine as the CLI, shares its rate-limit file, and draws with
Rune.

```bash
dep-check-gui
dep-check-gui --log-level debug
```

The flags only tune logging; the window has no command-line input.

## The window

- **Ecosystem**: Cargo, NuGet, PyPI, npm, or RubyGems. The choice lasts until the window closes.
- **Packages**: one per line, `name` or `name@version`. A package without a version is checked
  at the version deps.dev names as its latest, and its row says "(latest)". A line that cannot
  be checked is named under the box ("Line 3: ..."), and the status line says which line to
  fix; nothing is sent until every line reads.
- **Results**: one row per package with a badge (Malicious, Vulnerable, Clean, Not found, Not
  checked), its name and version, each finding (the id, its severity or "malware", the fixed
  versions, the summary, the other ids, the CVSS ratings and which services know it), and the
  supply-chain facts (licenses, deprecation, source repository, OpenSSF Scorecard, publish date,
  attestations). Over the rows, one line says what each service did; it turns to the warning
  color when one did not fully answer, because a "Clean" badge then means clean by the services
  that did. The links each finding carries are in the CLI's report and its JSON.
- **Check** runs the check. While it runs the button shows it is busy and a second press does
  nothing. The status line beside it says what is running, then how the check went.

A check of a thousand packages builds only the rows in view.

## Keys

| Key | Does |
|---|---|
| `Ctrl+Enter` (`Cmd+Enter` on macOS) | Check, from the editor, the picker, or the button |
| Return in Packages | a new line |
| Escape | closes the open ecosystem list; with nothing open it does nothing |

## Services and rate limits

dep-check-gui asks the same services the CLI does, with the same requests; the CLI's readme
says what each one gets. The rate-limit blocks live in
`~/.rusted-toolbox/dep-check/rate-limits.json`, shared with the CLI: a service the CLI was
blocked by is skipped here until the block ends, and the services line says so ("is rate
limited until ... (from an earlier run)"). When no service answered at all, the status line
says what to do next (when to try again, or to check the connection) and each service's reason.

Closing the window during a check drops the check. Each service's block is saved as that
service finishes; one still running at the close loses what it learned, which costs at most one
request that the service answers with another block.

## Log

The window logs at info to `~/.rusted-toolbox/dep-check-gui/logs`: a start line, the engine's
`check started` and `check finished` lines (counts and outcomes, never a package name), and
`check dropped` when the window closes during a check. `--log-level` changes the level;
`--log-level disabled` turns logging off.
