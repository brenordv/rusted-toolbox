# shared-eventhub-persistence

Everything the EventHub tools keep on disk: the message archive, search over
it, the txt/csv/json exporters, the disk-write modes, and `config.json` with a
local secrets store. Built on [redb](https://github.com/cberner/redb) 4.6.0
and `shared-eventhub-core`'s message model; the GUI and CLI never open redb
themselves.

## Data folder

```
<base>/
├── .gitignore         # "*", written when the folder is created or empty
├── archive.redb       # messages, time index, checkpoints, export log
├── secrets.redb       # connection string / client secrets
└── <received_msg_path>/<YYYY-MM-DD>/...   # disk-write dumps
```

`DataPaths` is the only code that knows this layout; `inbound_dir` and
`export_dir` place a configured folder in it (an absolute one as given). On
Unix the folder is created 0o700 and both redb files and every dump file
0o600 (an existing file is tightened when opened). On Windows the files
inherit the parent folder's ACL, which on some drives grants other local
users access. The `.gitignore` keeps `git add .` from ever picking up the
archive or the secrets file, so a folder that already holds files is used
only when its `.gitignore` is that same `*`; any other folder (a repository
root, a source folder with a `.gitignore` of its own) is refused with a
message asking for a new or empty folder.

### One event hub per data folder

`archive.redb` is bound to one event hub the first time a capture runs into
it. Reading another hub into the same data folder fails and names both hubs:
use another data folder. (The old `eh-read` kept one database per endpoint
inside a data folder.) Old `.eh-read-data` sled folders are not readable by
these tools and are never touched; captured data there stays readable through
the last released `eh-read`/`eh-export` 1.1.0.

A hub deleted and recreated under the same name counts as another hub: its
sequence numbers start over and would collide with the archived ones. The
binding records when the service created the hub, and a bind is refused when
the reported creation time differs from the recorded one (both times are
named), or when the archive holds an event enqueued more than a minute before
the reported creation time (the margin absorbs clock differences between the
service's management plane and its brokers). An archive bound before the
creation time was recorded gets it on its next bind, after that event check.
When the service reports no creation time, the bind goes ahead; it warns when
the archive already has a recorded time. Older builds ignore the recorded time
and bind by name.

## The archive

| Table | Key | Value |
|---|---|---|
| `meta` | name | `schema_version` ("1"), bound `namespace` and `event_hub`, the hub's `created_on` (RFC 3339 UTC, milliseconds) when known |
| `messages` | (partition, sequence) | serde_json of `CapturedMessage` |
| `by_time` | (enqueued unix ms, partition, sequence) | `()`: the time index |
| `checkpoints` | partition | serde_json of `Checkpoint` |
| `export_log` | (partition, sequence) | last export, unix ms |

Messages without an enqueue time are indexed under `i64::MIN`, so they sort
first.

Stored-format rule: values are serde_json of `shared-eventhub-core` types,
pinned by golden-bytes tests. Adding an `Option` field, or one with
`#[serde(default)]`, keeps schema version 1. A new `BodyKind` variant, a
rename, a removal, or a type change needs a new schema version; this build
refuses an archive with any version other than "1".

### Writing

`MessageStore` implements `MessageSink`. One writer thread owns capture
writes: it takes every request already queued (one per busy partition, since
each partition waits for its store), writes messages, time index, and
checkpoints in one transaction, and commits once at redb's default
`Durability::Immediate` before answering. Nothing is acknowledged before it is
on disk, so a crash loses nothing that was reported stored. Measured on a
Windows development machine (release build): about 850 events/s for a single
partition storing one event at a time; several partitions share each commit.

Storing an event at a key that already holds one:

- a different event (both enqueue times known and different) fails the store,
  with dedup on or off: the event hub was likely deleted and recreated,
  sequence numbers started over, and treating it as a duplicate or
  overwriting it would lose events. Use another data folder;
- otherwise, with dedup on (the normal run), it is a duplicate and nothing is
  written;
- with dedup off (`ignore_checkpoint`), the same event is overwritten.

A failed commit fails every store in that batch with the archive path and
cause, and every later write fails with the same cause until the archive is
reopened. A writer thread that dies fails stores with its cause.

### Opening and closing

`MessageStore::open` locks the archive file. A second open (another `eh`, an
`eh-gui`, or another handle in the same process) fails with a message that
says so. Handles are cheap clones; `close().await` on the last one stops the
writer, closes redb, and returns any error; dropping the last handle does the
same and only logs errors. The synchronous methods (`get`, `search`,
`export`, `export_one`) block; call them from `spawn_blocking` on a runtime.

The file never shrinks: there is no retention, purge, or compaction yet.

## Search

`search` takes `CaptureFilters` (time and/or text) and returns pages in
enqueue-time order. The time window narrows the index scan; the filters'
own `matches` decides, so search and capture apply exactly the same rules.
A message without an enqueue time matches no time filter. Each page reads its
own snapshot, so a message overwritten between pages can move across the
cursor.

## Export

`export` writes every match as txt (the details block or the body), csv
(`entity_path,partition_id,event_id,timestamp,message_content`, or
`message_content` alone), or json (the same fields as an object), one file per
message under `<folder>/<YYYY-MM>/<DD>/` or one condensed file per month.
Condensed json is always a complete pretty-printed array, and a csv file is
never mixed between the metadata and content-only layouts. A file that fails
is counted (`first_error` names one) and the export continues; a cancelled
export leaves every written file complete.

The export log is global, not per destination (as in the old tool): with
skipping on, exporting a range to csv and then to json exports nothing the
second time. An individual file that was deleted is exported again. A crash
between writing a page of files and logging them can export up to 512
messages twice, duplicating entries in condensed files.

CSV injection: `message_content` comes from whoever produced the event. A
cell starting with `=`, `+`, `-`, or `@` runs as a formula when the file is
opened in spreadsheet software.

`export_one` writes a single message to a chosen file (a copy; the export log
is not touched).

### Disk-write modes

`WritingSink` wraps the store as the engine's sink and writes newly stored
messages that pass the current capture filters to
`<inbound>/<YYYY-MM-DD>/<time>--<partition>-<sequence>.txt`:

- `Immediate`: as each message is stored. A write failure fails that store, so
  the partition ends `Failed`; the message is still in the archive, and
  `eh export` can write it out.
- `AtEnd`: when `finish()` is called after the run, keeping one key per
  matching message in memory until then.
- `Off`: archive only.

Call `set_filters` whenever the engine's filters change. Re-reading events
with `ignore_checkpoint` stores them again with a new capture time, so they
get new dump files, as with the old `--read-to-file`. A message whose dump was
never written (an `Immediate` failure, or a run future dropped between the
store and the dump) is still in the archive; `eh export` writes it.

## config.json and secrets

`config.json` in the working directory recalls a run's settings. It is only
read when it carries `"tool": "eh"`; any other `config.json` (including one
that is not JSON) is reported as `Foreign`, ignored, and never overwritten.
A load error never quotes what the file held: quoted values and the name in
an "unknown field" or "unknown variant" message are redacted, while the
valid names serde lists after "expected" stay.
Secrets never go into it:

```json
{
  "tool": "eh",
  "schema_version": 1,
  "target": {
    "namespace": null,
    "event_hub": "orders",
    "consumer_group": "$Default",
    "auth": { "method": "connection-string", "stored": "local-db" }
  }
}
```

`"stored": "local-db"` means the connection string is in `secrets.redb`.
Client secrets are stored under their client id. A Key Vault config keeps only
the vault URL and secret name; the fetched connection string is never written
anywhere. Saving removes stored secrets the new config no longer uses (the
freed bytes are not scrubbed), so keep one `config.json` per data folder: two
working directories saving into one shared data folder would undo each
other's secrets.

This keeps secrets out of plain sight and out of git, but it is not
encryption: anyone who can read the data folder can read them.

`needs_stored_secret` says whether resolving a config needs the secrets file
at all, `resolve_target_with` rebuilds the target from the given secrets and
the file, and `ToolConfig::missing_client_secret` then names the client whose
secret is still missing, as data rather than an error.
`ToolConfig::disk_write_config` and `engine_config` map a config to a capture
run's sink and engine settings.

Treat `config.json` and the data folder like a script: a cloned repo that
ships both could point `eh read` at someone else's event hub and dump folders.

## Workspace (profiles)

`workspace::Workspace` is a folder of saved connection profiles, one subfolder
each, laid out like a working directory (`config.json` beside its data
folder). `Workspace::locate` uses the absolute folder in `EH_WORKSPACE`, or
`<home>/.eh/profiles`; a relative `EH_WORKSPACE` is an error.

- `ProfileName::parse` allows 1 to 64 ASCII letters, digits, `-`, `_`, and
  `.`, starting with a letter or digit, not ending with `.`, and not a Windows
  device name (`CON`, `NUL.txt`, `com1`, ...). A name holds no separator or
  `..`, so it cannot leave the workspace. Its errors never quote a name that
  fails the character rule.
- `Workspace::profile` finds a folder whose name matches ignoring ASCII case
  (two matches are an error) and otherwise names `<root>/<name>`; it never
  creates anything.
- `Workspace::list` returns every subfolder holding an eh `config.json`,
  sorted ignoring case: `Ready` with the config, or `CannotLoad` with the
  redacted load error or a "rename it" reason for a folder name that is not a
  valid profile name. Files, broken links, and folders without an eh
  `config.json` are skipped. It logs one summary line, not one per profile.
- `ConfigHome` is where a run's `config.json` lives: the current folder or a
  profile. `data_paths` places a relative data folder inside the profile
  folder; under the current folder it stays relative, as it always was.
  `uses_recalled_folder` says whether a home uses a folder its `config.json`
  names (the data folder, the export folder, the received message path): a
  profile uses any folder, the current folder only a `plain_relative` one,
  because its `config.json` can come with a cloned repository. The check
  reads the text and never touches the folder. `data_paths`, `inbound_dir`,
  and `export_dir` do not apply it: a caller checks a recalled folder first
  and passes a folder the user typed as given.

Links inside the workspace are followed: only its owner writes there.

## Diagnostics

Events go through `tracing` and never carry message bodies or secrets:
`info` for archive open, the first bind, export runs, config load/save and a
writer summary on close; `warn` for failed export files (the first 20), slow
writes (over 2 s, at most once a minute), and writer failures; `trace` per
committed batch.
