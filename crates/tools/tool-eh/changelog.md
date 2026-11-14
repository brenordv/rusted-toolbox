# Changelog

## 1.0.0
- Initial release, replacing `eh-read` and `eh-export` 1.1.0 with one binary: `eh read`
  captures events into a redb archive per data folder, `eh export` writes archived events to
  txt, csv, or json.
- Built on the official Azure SDK (`azure_messaging_eventhubs` 0.16.0): connection string,
  Entra ID (developer tools, managed identity, client secret), and Key Vault auth.
- Time-window and regex filters on both subcommands; `--disk-mode off|immediate|at-end`.
- `config.json` recall with secrets kept in the data folder's `secrets.redb`;
  `EH_CONNECTION_STRING` and `EH_CLIENT_SECRET` environment variables.
- Exit codes 0 / 1 / 130; a partition that failed during a stopped run exits 1.
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
