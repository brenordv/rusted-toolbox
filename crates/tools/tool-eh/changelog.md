# Changelog

## 1.0.0
- Initial release, replacing `eh-read` and `eh-export` 1.1.0 with one binary: `eh read`
  captures events into a redb archive per data folder, `eh export` writes archived events to
  txt, csv, or json.
- Built on the official Azure SDK (`azure_messaging_eventhubs` 0.16.0): connection string,
  Entra ID (developer tools, managed identity, client secret), and Key Vault auth.
- Time-window and regex filters on both subcommands; `--disk-mode off|immediate|at-end`. The
  timestamp parsing is `shared-eventhub-core`'s, so `--during` with a date alone suggests the
  whole-day window as "from ...T00:00:00Z to ...T23:59:59.999Z".
- `config.json` recall with secrets kept in the data folder's `secrets.redb`;
  `EH_CONNECTION_STRING` and `EH_CLIENT_SECRET` environment variables.
- Exit codes 0 / 1 / 130; a partition that failed during a stopped run exits 1.
- `eh send` publishes one body (`--body`, `--body-file`, or stdin) `-n` times, batched or one
  every `--interval-ms`, with `--partition-key` and `--content-type`. A target recalled from
  `config.json` must be named again (`--namespace` and `-e`) or confirmed with `--yes`. `-g/--consumer-group` is a `read` flag only.
- `eh send --template` treats the body as a template: `{{type option=value}}` placeholders get
  fresh mock values (mock-data-utils) in every event; compile errors name the line and column
  and stop the run before anything is opened. `--dry-run` prints the rendered events instead of
  sending, with no event hub and no `config.json` read. The header gains a Template line and
  `send --help` lists the placeholders. `{{now}}` and `{{utc-now}}` put the time each event is
  built into it (RFC 3339 to the millisecond, local with its offset or UTC with `Z`).
  `escape=json` on a placeholder escapes its value for a JSON string (`"`, `\`, U+0000 to
  U+001F), so a pt-br `D"ávila` stays valid JSON; `send --help` lists it.
- The export progress line (and the at-end message file line of `eh read`) is cleared when the
  summary follows, so the final counts print once; a failed export keeps its last counts.
- `--dump-filter` follows eh-gui's keyword rule: each keyword is trimmed and empty items are
  dropped, so a trailing or doubled comma (`--dump-filter error,`) no longer matches every
  message, and a list with no keyword left (`--dump-filter ,`) is refused before anything is
  opened. A keyword filter saved in `config.json` loads through the same rule, and one with no
  keyword left is a `config.json` load error. `--content-type` is trimmed, and a blank value
  sends no content type instead of an empty one.
- `eh send --property KEY=VALUE` (repeatable) adds a string application property to every
  event and `--correlation-id ID` sets its correlation id (trimmed; blank sets none). A malformed
  or repeated property fails before anything is opened, naming the flag's position and never a
  value; neither flag is saved to `config.json` or allowed with `--dry-run`. `--app-header` shows
  the correlation id and up to five property keys.
- Breaking, from 1.1.0 (the readme has the full table):
  - `eh-read` -> `eh read`, `eh-export` -> `eh export`; shared flags go before the
    subcommand.
  - `-c/--config` -> `config.json` + `--save-config`.
  - `-p -1` -> omit `-p`; `-p` repeats.
  - `-f/--read-to-file` -> `--disk-mode immediate`.
  - default data folder `.eh-read-data` -> `.eh-data`; sled data is not migrated.
  - `-d`, `-x`, `--export-base-data-folder` removed.
  - export `-i/--ignore-checkpoint` -> `-i/--ignore-export-log`.
  - export `-e/--export-folder` -> `--export-folder`.
  - `--include-metadata` takes `true|false`.
