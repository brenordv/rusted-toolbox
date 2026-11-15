# eh

`eh` reads events from an Azure Event Hub into a local archive and exports them to files.

- `eh read` captures events from one event hub (every partition, or the ones you pick),
  stores each one in the data folder's archive, and resumes from where it stopped on the
  next run. It can filter what it shows by time window or text, and write matching messages
  to files as they arrive or when the run ends.
- `eh export` writes archived events to txt, csv, or json files, with the same filters. It
  never connects to Azure.

`eh` replaces `eh-read` and `eh-export` 1.1.0 (see "Coming from eh-read and eh-export").

The shared flags (`--app-header`, `--verbose`, `--log-level`, `--log-to-file`, ...) go
**before** the subcommand: `eh --verbose read ...`. After it, they are a usage error.

## Reading

### Authenticate

Pick one of four ways. Quote values in your shell: a connection string contains `;`, which
ends the command in bash and PowerShell, and an unquoted `$Default` expands to nothing.

```bash
# Connection string from the environment (keeps it out of shell history)
export EH_CONNECTION_STRING='Endpoint=sb://my-ns.servicebus.windows.net/;SharedAccessKeyName=listen;SharedAccessKey=...;EntityPath=orders'
eh read

# Connection string on the command line (visible in the process list and history)
eh read -s 'Endpoint=sb://my-ns.servicebus.windows.net/;SharedAccessKeyName=listen;SharedAccessKey=...' -e orders

# Entra ID: your Azure CLI / Azure Developer CLI login, or the machine's managed identity
eh read --auth developer --namespace my-ns.servicebus.windows.net -e orders
eh read --auth managed-identity --namespace my-ns.servicebus.windows.net -e orders

# A service principal; the secret only ever comes from EH_CLIENT_SECRET (or a saved config)
export EH_CLIENT_SECRET='...'
eh read --auth client-secret --tenant-id <tenant> --client-id <client> --namespace my-ns.servicebus.windows.net -e orders

# A connection string kept in Key Vault; --auth is the credential used for the vault
eh read --auth developer --keyvault-url https://my-vault.vault.azure.net --keyvault-secret eh-orders
```

The namespace must end in `servicebus.windows.net`, `servicebus.chinacloudapi.cn`, or
`servicebus.usgovcloudapi.net`. `managed-identity` is the system-assigned identity; a
user-assigned identity is not supported yet. A SAS key in a connection string can expire
or be rotated; the run then fails to connect, and you need the new string.

### Consumer group

`-g/--consumer-group` defaults to `$Default` (quote it: `-g '$Default'`). Use a consumer
group of your own: Event Hubs allows five readers per partition and consumer group, and a
reader with an owner level (an `EventProcessor`, for example) on the same group disconnects
`eh`. A consumer group that does not exist is only noticed when the partitions start, so
the run fails per partition.

### What a run looks like

```
Connecting to namespace my-ns.servicebus.windows.net, event hub orders (consumer group eh-local)...
Reading partitions: 0, 1, 2, 3
Received: 1520 | Stored: 1518 | Duplicated: 2 | Filtered: 300 | Rate: 41.20 msg/s | Runtime: 00:00:37.012 | Last: 14:03:11.204
```

The progress line updates every `--feedback-interval` seconds (default 1). `--verbose`
adds a one-line preview per message. Notices (connecting, partitions starting or ending)
go to stderr; the progress line and the final summary go to stdout. A log line can land in
the middle of the progress line; the next redraw fixes it.

Ctrl-C stops every partition gracefully and prints the final summary: what was received,
stored, duplicated, filtered out, how each partition ended, and the at-end message files.
Every stored message is already on disk when it is counted. Stopping waits for the SDK to
close its connections; a second Ctrl-C does nothing, so if a stop hangs, close the terminal.
Nothing is lost. Ctrl-C while still connecting ends the run at once ("Interrupted while
connecting; nothing was captured").

### Resuming and the checkpoint

Every stored event updates its partition's checkpoint, and the next run starts right after
it. The checkpoint wins over a time filter's lower bound: when the filter window lies
entirely before the checkpoint, the run warns and reads nothing new. Use
`-i/--ignore-checkpoint` to read from the filter's start (or the earliest retained event)
and store everything again, or `eh export` to query what is already archived.

Events older than the hub's retention are gone; a checkpoint behind them resumes at the
earliest retained event. When a hub is deleted and recreated, sequence numbers start over,
so its archive can't take the new hub's events. The archive records when the service created
the event hub, and a run against a hub of the same name created at another time stops before
reading anything: "The event hub 'ns/hub' was created on 2026-11-14 21:30:05.123 UTC; this
data folder's archive holds the one created on 2026-11-13 18:02:41.870 UTC: the event hub was
deleted and recreated (or replaced). Use another data folder." An archive that holds events
enqueued before the hub's creation time is refused the same way, which also covers an archive
from before the creation time was recorded.

When the service doesn't report the creation time, two later checks remain: a partition whose
checkpoint is ahead of the new hub fails at start ("The stored checkpoint of partition 0
(sequence number 24) is ahead of the event hub (last sequence number -1). The event hub was
likely deleted and recreated. Use another data folder."), and with `-i`, storing an event at
a sequence number the archive already holds for a different event fails that partition the
same way. A recreated hub that has already passed the old checkpoint is then not detected:
its earlier events are skipped.

### Filters

| Flag | Keeps events |
|---|---|
| `--older-than T`, `--older-or-equal T` | enqueued before (or at) `T` |
| `--newer-than T`, `--newer-or-equal T` | enqueued after (or at) `T` |
| `--during T` | in the hour, minute, or second `T` names: `12:00:00` is the hour, `12:34:00` the minute, `12:34:56` the second |
| `--between A..B` | enqueued from `A` to `B`, both included |
| `--dump-filter a,b` | whose body contains `a` or `b`, ignoring case |
| `--filter-regex P` | whose body matches the regular expression `P`, ignoring case unless `P` starts with `(?-i)` |

Times are RFC3339 (`2026-07-02T12:34:56Z`, `2026-07-02T14:34:56+02:00`) or a date
(`2026-07-02`, midnight UTC); `--during` needs a full timestamp. One time filter and one
text filter per run.

Filters decide what is shown and written to message files, never what is archived: every
event is stored first and filtered afterwards. An event without an enqueue time never
matches a time filter.

### Message files (`--disk-mode`)

- `off` (default): the archive only.
- `immediate`: each new matching message is written as it is stored. A write failure
  fails its partition; the message is still in the archive.
- `at-end`: every new matching message is written when the run ends.

Files go to `<data folder>/<received-msg-path>/<YYYY-MM-DD>/<capture time>--<partition>-<sequence>.txt`
(`-r/--received-msg-path`, default `inbound`, relative to the data folder unless absolute).
`--dump-content-only` writes only the body. Re-reading with `-i` stores events again with a
new capture time, so they get new files.

### Partitions and prefetch

`-p/--partition` picks partitions (repeat it); the default is all. Receivers use the SDK's
default prefetch of 300 events; `capture.prefetch` in `config.json` changes it.

## The data folder

`-b/--base-data-folder` (default `.eh-data`) holds `archive.redb` (messages, checkpoints, the
export log), `secrets.redb`, and the message files. One data folder holds one event hub:
reading another hub into it fails at start and names both. Use another folder per hub.

A new data folder gets a `.gitignore` of `*`. A folder that already holds files is used only
when its `.gitignore` is that same `*`, so the archive and secrets never land in a folder git
tracks (a repository root, a source folder). The archive never shrinks yet (no purge); see
the [persistence readme](../../libs/shared-eventhub-persistence/readme.md) for its layout,
the CSV-injection caveat, and how storing behaves.

Data captured by `eh-read` 1.1.0 (sled, in `.eh-read-data`) is not read or touched by `eh`;
the 1.1.0 binaries still read it.

## Saving settings: config.json

`eh read --save-config ...` writes the run's settings to `config.json` in the current
folder, then runs. The next `eh read` or `eh export` in that folder recalls them, and says
so on stderr ("Using config.json: ..."); flags given on the command line win. `--no-config`
ignores the file.

```json
{
  "tool": "eh",
  "schema_version": 1,
  "target": {
    "namespace": "my-ns.servicebus.windows.net",
    "event_hub": "orders",
    "consumer_group": "eh-local",
    "auth": { "method": "connection-string", "stored": "local-db" }
  },
  "capture": { "partitions": null, "ignore_checkpoint": false, "prefetch": null, "feedback_interval_secs": 1, "filters": { "time": null, "text": null } },
  "storage": { "base_data_folder": ".eh-data", "disk_write": { "mode": "off", "received_msg_path": "inbound", "content_only": false }, "export": { "format": "txt", "include_metadata": true, "condense_output": false, "use_local_time": false, "export_folder": "exports", "skip_already_exported": true } }
}
```

- Secrets never go into `config.json`. A connection string or client secret (from a flag or
  from `EH_CONNECTION_STRING`/`EH_CLIENT_SECRET`) is saved in the data folder's
  `secrets.redb`, and `eh` says so. That file is not encrypted: anyone who can read the data
  folder can read it. Removing a secret does not scrub its old bytes; after rotating a key,
  delete `secrets.redb` and save again.
- A Key Vault config keeps only the vault URL and secret name.
- Keep one `config.json` per data folder: saving drops secrets the new config does not use.
- Switches (`-i`, `--dump-content-only`, and export's `--condense-output`,
  `--use-local-time`, `-i`) only turn a setting on; to turn a recalled one off, use
  `--no-config` or save again.
- Treat `config.json` like a script: a cloned repository could ship one that points `eh` at
  another event hub or folder. The notice shows what it recalled.
- A `config.json` without `"tool": "eh"` belongs to another tool: `eh` ignores it (with a
  notice) and never overwrites it.

## Exporting

```bash
eh export
eh export -f csv --include-metadata false --condense-output
eh export -f json --between 2026-07-02T00:00:00Z..2026-07-02T23:59:59Z --filter-regex 'order-\d+'
```

`eh export` reads `<data folder>/archive.redb` (`-b`, or the recalled data folder) and fails
when there is none. Files go to `--export-folder` (default `exports`, relative to the data
folder unless absolute; a path with `..` can leave the git-ignored folder):

- txt: the details and the body (`--include-metadata false`: the body only).
- csv: `entity_path,partition_id,event_id,timestamp,message_content`, or `message_content`
  alone without metadata. `entity_path` is the archive's event hub.
- json: the same fields as an object.

One file per message under `<YYYY-MM>/<DD>/`, or one file per month with
`--condense-output`; dates in UTC unless `--use-local-time`.

The export log is global: a message exported once, in any format or folder, is skipped by
later exports unless `-i/--ignore-export-log` is given (a deleted single file is exported
again). Ctrl-C stops between messages; every file written is complete. Export settings come
from flags, then the recalled `storage.export`; the recalled capture filters do not apply.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | completed (`export` wrote everything; `read` reached the end of a time window) |
| 1 | any failure: bad arguments or config, connection, a partition that failed (even in a run stopped with Ctrl-C), message files that could not be written, the archive in use or failing to close |
| 2 | a usage error caught while parsing the flags |
| 130 | stopped with Ctrl-C, with nothing failed |

## Troubleshooting

`eh --log-level debug --log-to-file read ...` writes connection, partition, and archive
details to a log file without disturbing the progress line. Log output never contains
message bodies or secrets.

## Coming from eh-read and eh-export

| 1.1.0 | eh 1.0.0 |
|---|---|
| `eh-read ...` | `eh read ...` |
| `eh-export ...` | `eh export ...` |
| `-c/--config <file>` | `config.json` in the current folder, saved with `--save-config` |
| `-v/--verbose` | `eh --verbose read` |
| `-p/--partition-id <n>`, `-1` for all | `-p/--partition <id>`, repeatable; omit for all |
| `-f/--read-to-file` | `--disk-mode immediate` (or `at-end`) |
| `-b`, default `.eh-read-data` | `-b`, default `.eh-data` (old data is not migrated) |
| `-d/--database-path`, `-x/--export-database-path`, `--export-base-data-folder` | gone: one `archive.redb` per data folder |
| export `-i/--ignore-checkpoint` | `-i/--ignore-export-log` |
| export `-e/--export-folder` | `--export-folder` |
| export `--include-metadata` (could not be turned off) | `--include-metadata true\|false` |
| export `--feedback-interval` | gone (the export line redraws on its own) |
| `-s`, `-e`, `-g`, `-r`, `--dump-filter`, `--dump-content-only`, `--feedback-interval`, `-i`, `--condense-output`, `--use-local-time`, `-f/--export-format` | unchanged |

New in `eh`: Entra ID and Key Vault auth, time and regex filters (also on export), `at-end`
message files, the `EH_CONNECTION_STRING`/`EH_CLIENT_SECRET` environment variables, and a
final summary with how each partition ended.
