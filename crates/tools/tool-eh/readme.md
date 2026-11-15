# eh

`eh` reads events from an Azure Event Hub into a local archive, exports them to files, and
sends test events.

- `eh read` captures events from one event hub (every partition, or the ones you pick),
  stores each one in the data folder's archive, and resumes from where it stopped on the
  next run. It can filter what it shows by time window or text, and write matching messages
  to files as they arrive or when the run ends.
- `eh export` writes archived events to txt, csv, or json files, with the same filters. It
  never connects to Azure.
- `eh send` publishes one body to an event hub a given number of times, as fast as batches
  go or one event every N milliseconds.

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

`eh read --save-config ...` (or `eh send --save-config ...`) writes the run's settings to
`config.json` in the current folder, then runs. The next `eh read`, `eh export`, or `eh send`
in that folder recalls them, and says
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

## Sending

```bash
eh send -e orders --body '{"orderId": 1}' --content-type application/json
eh send --body-file order.json -n 500
eh send --body-file - --partition-key customer-17 < event.json
eh send -e orders --body ping -n 600 --interval-ms 1000
```

`eh send` takes the same target and auth flags as `read` (and `--save-config`, `-b`,
`--no-config`), but no consumer group.

| Flag | Meaning |
|---|---|
| `--body TEXT` | the body, sent as UTF-8 |
| `--body-file PATH` | the body read from a file, byte for byte; `-` reads stdin. 20 MiB at most |
| `-n, --count N` | how many events, 1 (default) to 1,000,000 |
| `--partition-key KEY` | every event carries KEY: all land in one partition, in order. Without it, the service spreads events over partitions |
| `--content-type TYPE` | the content type of every event |
| `--interval-ms MS` | one event every MS milliseconds (1 to 3,600,000) instead of full batches |
| `--yes` | send to a target recalled from `config.json` without naming it again |

The run prints where it sends before the first event ("Sending 500 event(s) to
my-ns.servicebus.windows.net/orders, automatic routing..."), a progress line, and a summary.
`--count` and `--interval-ms` are never saved to `config.json`.

**A recalled target must be named.** A cloned repository can ship a `config.json` that points
at someone else's event hub, and with Entra ID auth your own identity would write there. So in
a folder with a `config.json`, `send` refuses to run until the destination comes from you or
`--yes` confirms it: a connection string you give (`-s` or `EH_CONNECTION_STRING`), a vault you
name (`--keyvault-url`), or `--namespace` and `-e` together. A saved connection string or Key
Vault secret must then match both, so a string without an `EntityPath` can't keep its own
namespace under an `-e` you typed.

**Rights, least privilege first.** Give an Entra ID identity the Azure Event Hubs Data Sender
role at event hub scope, or use a SAS rule with only the Send right on the event hub. Never use
the namespace's `RootManageSharedAccessKey`. A Listen-only rule saved for `read` fails at the
first batch. One `config.json` serves both subcommands, so a folder that both reads and sends
is better served by Entra ID (Data Sender plus Data Receiver) or two data folders than by a
broader key: `secrets.redb` is not encrypted. Role assignments can take up to five minutes to
apply.

**Sizes and throughput.** The SDK sizes a batch's first event twice, so one event can use about
half the publish limit: about 128 KB on Basic and 512 KB on Standard and Premium (whose limits
are 256 KB and 1 MB). A larger body fails with that explanation. Basic and Standard take 1 MB/s
or 1,000 events/s per throughput unit; past that the service throttles, the SDK retries a
batch for up to about a minute, and the "Send rejected by remote" warnings are those retries.

**What "sent" means.** `Sent` counts events the service accepted or did not refuse (the SDK
treats a "modified" or "released" outcome as success and warns "not durably accepted"; such
events may be missing). A retry after a lost acknowledgement can store an event twice, with the
same message id, and `eh read` then archives both. Sent events are not written to the archive;
`eh read` captures them like any other event.

**Pacing.** `--interval-ms N` sends one event per batch, each send starting N ms after the
previous one started. A slow send delays the next event and never causes a burst to catch up.
The interval is a floor: throttling, SDK retries, and a reconnect after a long idle wait stretch
it. The progress line updates once per event.

**Stopping.** Ctrl-C stops before the next batch, or within about 100 ms during a paced wait. A
batch already handed to the SDK finishes first, which can take about a minute while it retries;
`eh` then says "Stopping after the batch in flight". Ctrl-C while connecting ends the run at
once ("Interrupted while connecting; nothing was sent").

**Network.** AMQP over TCP, port 5671. The SDK version in use has no WebSocket transport, so a
network that blocks 5671 blocks `eh send` (and `eh read`). A connection string with a pre-formed
`SharedAccessSignature` cannot be renewed and stops working when its token expires.

Troubleshooting throttling or slow sends:
`RUST_LOG=warn,azure_messaging_eventhubs=debug,shared_eventhub_core=debug eh send ...`.

### Templates (`--template`) and dry runs (`--dry-run`)

With `--template`, the body is a template: every `{{type option=value}}` placeholder gets a
fresh mock value in each event, from the same generators as the `mock` tool.

```bash
eh send --template --dry-run -n 3 --body-file order-template.json
eh send -e orders --template -n 500 --content-type application/json --body \
  '{"orderId": {{random.integer min=1000 max=9999}}, "email": "{{person.email}}", "placedAt": "{{random.datetime past}}"}'
```

- A placeholder names a type (`person.email`, `random.integer`, ...; `eh send --help` lists all
  32) and takes options as words: `min=N max=N` (`random.integer`, `random.float`),
  `precision=N`, `length=N` (up to 1048576), `age=N`, `range=N`, `past`, `future`, and
  `locale=CODE` (`pt-br`, `ja-jp`, ...). Options a type ignores are ignored; a locale a type
  ignores prints a notice. Whitespace and newlines inside the braces don't matter.
- `{{now}}` and `{{utc-now}}` take no options and put in the time the event is built, RFC 3339
  to the millisecond: `{{now}}` in this machine's time zone with its offset
  (`2026-11-15T14:03:07.123-03:00`), `{{utc-now}}` in UTC (`2026-11-15T17:03:07.123Z`). Every one
  in an event shows the same instant. `{{now}}` reveals this machine's UTC offset; use
  `{{utc-now}}` to leave it out. Compared with the enqueued time the service stamps, expect the
  skew between this machine's clock and the broker's.
- `\{{` writes a literal `{{`; any other backslash stays as written. So a backslash right before
  a placeholder can't be written: `logs\\{{internet.username}}` sends
  `logs\{{internet.username}}`.
- Values go in as generated, with no escaping. A JSON template breaks when a value holds `"` or
  `\`, and some do: Brazilian last names include `D"ávila` (`{{person.last-name locale=pt-br}}`).
- The body must be UTF-8. A template that does not compile fails before anything is opened or
  saved, naming the line and column ("The --template body does not compile: line 3, column 14:
  unknown type 'person.fullname'"). A template holds at most 100,000 placeholders, and each
  event at most 20 MiB once rendered.
- Every event is rendered as it is sent, so values are fresh per event and not reproducible.
  Without `--template`, `{{` is sent as written.

`--dry-run` (with `--template`) renders `--count` events and prints them instead of sending: no
event hub, no target flags, and no `config.json` are read, and nothing is written. Each event's
lines print with control characters escaped, a blank line between events, and stderr ends with
"Dry run: 3 event(s) rendered, nothing sent." It can't be combined with `--save-config`,
`--partition-key`, `--interval-ms`, or `--yes`. Piping into `head` ends it quietly; Ctrl-C stops
it between events (exit 130). `--app-header` prints its lines before the events, on stdout.

### Live checks (manual, against a dev event hub)

The test suite never contacts Azure. Before relying on `send`:

1. One event, then `eh read` shows it; `--content-type application/json` shows in
   `eh export -f json`.
2. `-n 1000`: count, batches, and rate in the summary.
3. `-n 20000` against one throughput unit: note the "Send rejected by remote" warnings, the
   backoffs with the `RUST_LOG` line above, and how long Ctrl-C takes mid-batch.
4. `--partition-key k`: every event lands in one partition.
5. `-n 10 --interval-ms 1000 --partition-key k` arrives about one a second, in order, in a
   running `eh read`; Ctrl-C during it stops within a second.
6. A Listen-only connection string fails at the first batch (note the text).
7. A 150 KB body on Basic and a 600 KB body on Standard fail with the about-half message; a
   120 KB body on Basic is sent.
8. `eh read` with a Send-only connection string: whether reading the event hub properties
   succeeds tells whether a Send-only rule can read them (note the result here).
9. The time from "Connecting" to the "First batch created" log line (`--log-level info`).
10. `--template -n 5` with the order template above: `eh export -f json` shows five different
    order ids and emails.
11. `--template --dry-run -n 1000 ... | head -3` ends quietly with exit 0 (on Windows too).

## Exit codes

| Code | Meaning |
|---|---|
| 0 | completed (`export` wrote everything; `read` reached the end of a time window; `send` sent every event) |
| 1 | any failure: bad arguments or config, connection, a partition that failed (even in a run stopped with Ctrl-C), message files that could not be written, the archive in use or failing to close, a send that failed or a producer that failed to close |
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
