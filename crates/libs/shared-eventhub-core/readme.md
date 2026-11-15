# shared-eventhub-core

UI-agnostic Event Hubs capture and sending: the captured-message model, capture
filters, the connection to an event hub, the capture engine, live statistics,
and a producer for test events.
Nothing here prints, renders, or touches the filesystem; storage happens
through the `MessageSink` trait, and consumers talk to the engine through
channels. Built on the official `azure_messaging_eventhubs` SDK (0.16.0);
credentials and Key Vault come from `shared-azure`.

## Connecting

`EventHubTarget` names the event hub, the consumer group, and how to
authenticate (`shared_azure::auth::AuthMethod`):

- `ConnectionString`: namespace and hub come from the string (`Endpoint`,
  `EntityPath`). A hub name or namespace configured alongside must match.
- `Credential`: an Entra ID credential; the fully qualified namespace and the
  hub name are required.
- `KeyVaultConnectionString`: the connection string is fetched from Key Vault
  first, then used as above. It is never stored by this crate.

A `ClientSecret` credential kind takes its secret from
`EventHubTarget::client_secret`, for either the event hub or the vault.

`connect` validates before any network call, then opens the client and reads
the event hub's properties. Opening only establishes the AMQP connection, so
that read is what proves the credential works; its partition list also
validates any requested partitions.

Validation rules:

- The namespace host must sit under an Event Hubs DNS suffix on a label
  boundary: `servicebus.windows.net`, `servicebus.chinacloudapi.cn`, or
  `servicebus.usgovcloudapi.net`. A tool may load its target from a config
  file in the working directory, and an Entra token presented to a foreign
  host could be replayed against any namespace the identity can reach.
- Event hub names follow the Event Hubs naming rules (1 to 256 letters,
  digits, `.`, `-`, `_`, starting and ending with a letter or digit). A value
  that is not shaped like a namespace or hub name is rejected without being
  echoed, so a key pasted into the wrong field never lands in an error.
- Errors name the namespace and hub; none carries the connection string or a
  key.

SAS tokens: a connection string with `SharedAccessKeyName`/`SharedAccessKey`
signs and refreshes its own tokens. One that carries a pre-formed
`SharedAccessSignature` cannot be renewed, so a long capture ends when that
token expires. Prefer key-based strings or credential auth for long runs.

## Consumer groups

Use a dedicated consumer group for capture. Event Hubs allows five concurrent
readers per partition per consumer group, and a sixth surfaces as a generic,
retried error rather than a named one. The engine opens non-epoch receivers,
so it never displaces another consumer; but a consumer with an owner level
(an `EventProcessor`, for one) on the same group disconnects every non-epoch
reader on its partitions, and the engine reports that partition as taken
over. See
[consumer groups](https://learn.microsoft.com/azure/event-hubs/event-hubs-features#consumer-groups)
and [quotas](https://learn.microsoft.com/azure/event-hubs/event-hubs-quotas).

## Filters

`CaptureFilters` holds an optional time filter and an optional text filter;
when both are set, both must pass.

Time filters compare against the broker-assigned enqueue time, never the local
receive time, so local clock skew does not move a window. A message without an
enqueue time fails every time filter.

| Filter | Matches enqueue time `t` |
|---|---|
| `OlderThan(x)` | `t < x` |
| `OlderOrEqual(x)` | `t <= x` |
| `NewerThan(x)` | `t > x` |
| `NewerOrEqual(x)` | `t >= x` |
| `Between { start, end }` | `start <= t <= end` |
| `During(x)` | the second, minute, or hour named by `x` (below) |

`During` derives its window from the finest non-zero part of the timestamp:

| `During` value | Window |
|---|---|
| `2026-07-02T12:34:56.000Z` | that second |
| `2026-07-02T12:34:00.000Z` | that minute |
| `2026-07-02T12:00:00.000Z` | that hour |

So the rule cannot express "the minute 12:00" or "the second 12:34:00"; use
`Between` for those.

Text filters match the message body. `Contains` is a case-insensitive
substring match against any of its strings (an empty list matches nothing).
`TextFilter::regex` compiles a case-insensitive regex; `(?-i)` inside the
pattern turns case sensitivity back on. The regex crate matches in linear time
with no backtracking, so any user-supplied pattern is safe to run against
every message without a timeout.

## The capture engine

`CaptureEngine::build(config, sink)` returns the consumer's channels and the
run future. Run the future on a tokio runtime; it resolves once a `Stop`
command arrives or no partition task remains, and returns an error only when
the run cannot start (connection, partition selection, or the sink refusing
`MessageSink::bind`). After connecting, the engine calls `bind` once with the
validated namespace and hub, before any partition loads a checkpoint, so a
sink can refuse to mix two event hubs' data. The summary's
`RunEnd` is `Stopped` after a stop, `Failed` (naming the partitions) when any
partition failed, and `Completed` otherwise.

`Stop` is the graceful end: it is honoured during each partition's setup too,
and never interrupts a store. Dropping the run future instead aborts the
partition tasks mid-flight, so `MessageSink::store` must be cancel-safe.
Consumers must read the event channel independently of sending commands;
control events wait for channel space, so a consumer stuck on a full command
channel while not reading events stalls the run.

Each partition task:

1. Picks its start: the stored checkpoint (resumed after, exclusively) beats
   the filter's lower bound (an inclusive enqueue-time start, floored to
   milliseconds, so the service never skips an event the filter wants), which
   beats the earliest retained event. A lower bound before 1970 starts at the
   earliest event.
2. Reads the partition's properties and, when the time window is already
   closed, ends without reading if the partition is empty or the start is past
   its last event; otherwise it reads up to that last event and ends.
3. Stores every event together with its checkpoint, then applies the filters:
   a passing event is emitted as `Message`, any other as `Skipped`.

**Capture filters affect what is emitted, not what is stored.** Every event is
stored, so the sink stays a complete archive; the filters decide what the
live session shows. Changing filters mid-run applies to events processed
after the change.

Events and statistics:

- `Message` and `Skipped` events are dropped when the consumer lags and
  counted in `StatsSnapshot::ui_dropped`; capture and storage never wait for
  the view.
- `Started`, `PartitionEnded`, and `Error` always arrive.
- Statistics arrive on a `watch` channel holding the latest snapshot, at the
  configured feedback interval (at least 100 ms). Per-partition receive and
  enqueue times show whether one partition is stuck or just behind.

### Checkpoints and replays

- After a reconnect, the SDK re-attaches the receiver with its original start
  position and re-delivers events. Each partition task skips events at or
  below the highest sequence number it has already processed, so a replay is
  never stored or emitted twice, with or without `ignore_checkpoint`.
- A checkpoint trimmed away by retention resumes at the earliest retained
  event. A checkpoint ahead of the partition's last sequence number (usually
  an event hub deleted and recreated) fails that partition at start, naming
  both numbers, since the service refuses a start there. A time window that
  ends before the checkpointed event reads nothing and logs a warning
  suggesting the checkpoint be ignored; the archive may already hold those
  messages.
- `ignore_checkpoint` starts from the filter bound or the earliest event and
  stores every event again, but never moves the stored checkpoint backwards:
  events at or below it are re-stored with the existing checkpoint, so the
  next normal run still resumes where the last one left off. The exception is
  a checkpoint ahead of the partition (a recreated event hub), which the
  re-read replaces.

### Limits

- A time window that ends in the future keeps a partition open until an event
  enqueued after the window arrives. That exit assumes enqueue times rise
  with sequence numbers inside a partition; the Event Hubs documentation
  guarantees ordered delivery and rising sequence numbers but does not state
  this for enqueue times, so a late event with an earlier timestamp would be
  missed.
- A window counts as closed when its end lies before the partition's last
  enqueue time or before the local clock. For a window that ends at or near
  now, leave a few seconds of margin for clock skew between the broker and
  this machine.
- A live filter change to an earlier upper bound only affects partitions that
  have not already passed it.
- The SDK retries a failed connection for about a minute (8 retries, 60 s in
  total). A longer outage ends the affected partitions, even in an unbounded
  run; restart the run to resume from the checkpoints.

## Sending

`open_producer(&target)` validates the same `EventHubTarget` as `connect`
(the consumer group is ignored) and opens a producer. Opening only
establishes the AMQP connection; the credential is first used when the first
batch is created. `EventHubProducer::fully_qualified_namespace` and
`event_hub_name` are the resolved names, so a connection-string target can
show its hub before anything is sent. Always `close()` the producer: dropping
the SDK client does not reliably close its connection.

`EventHubProducer::send(&plan, next_event, &cancel, progress)` sends
`plan.count()` events and returns a `SendSummary` (`sent`, `batches`,
`elapsed`, and `SendEnd::Completed`, `Cancelled`, or `Failed(reason)`):

- `next_event(index)` builds each `OutgoingEvent` (body and content type);
  `OutgoingEvent`'s `Debug` shows the body length only.
- `SendPlan::new(count, routing, pacing)` takes 1 to 1,000,000 events.
  `Routing::automatic()` lets the service pick partitions;
  `Routing::partition_key(key)` sends every event with that key (one
  partition, in order).
- `Pacing::batched()` packs as many events per batch as fit.
  `Pacing::interval(d)` (1 ms to one hour) sends one event per batch, each
  send starting `d` after the previous one started; a slower send delays the
  next one and never causes a burst.
- `progress` runs after every sent batch (once per event when paced).
- Setting `cancel` stops before the next batch, while a batch is being
  created, or during a paced wait (within about 100 ms). A batch already
  handed to the SDK finishes first, which can take as long as the SDK's
  retries (about a minute).
- The first failure ends the run: an event that does not fit an empty batch
  (the SDK sizes a batch's first event twice, so one event can use only about
  half the publish limit: about 128 KB on Basic, 512 KB on Standard and
  Premium), a failed batch creation, a failing `next_event`, or a send the SDK
  gave up on. Nothing is retried on top of the SDK's own retries, and a failed
  batch is not counted.
- `sent` counts events in batches the service accepted or did not refuse:
  the SDK treats `Modified` and `Released` outcomes as success with a warning.
  A send retried after a lost acknowledgement can store an event twice, with
  the same message id.

`cancel::flag_set(&flag)` resolves once a flag is set, polling every
`CANCEL_POLL` (100 ms); it suits a race that ends on the flag.

## Logging

The engine logs connect and partition lifecycle at info, start decisions and
unkeyable events at debug, and per-event storage at trace (keys only, never
message content). The SDK logs its own lifecycle at info and failures at
warn/error. A completed reconnect is info, so it is invisible at the default
warn level. To see reconnects and engine detail without hiding anything else:

```
RUST_LOG=warn,azure_messaging_eventhubs=info,shared_eventhub_core=debug
```

The producer logs the open and the first batch creation (when the sender is
authorized) at info, each sent batch at trace (counts, bytes, send and wait
times, never the body or the partition key), the run's end at info, and the
close at debug.

A bare `RUST_LOG=azure_messaging_eventhubs=debug` replaces the whole filter and
hides the tools' own warnings. A failed receive shows up twice, once as the
SDK's log line and once as the engine's `Error` event.

Never enable the `azure_core` `debug` feature (it turns off the SDK's
redaction of payloads and properties) or the `fe2o3-amqp` `tracing`/`log`
features (they trace every AMQP frame, token messages included). Cargo merges
features across a workspace, so a new dependency must not turn them on.

## Manual verification (not CI)

The test suite never contacts Azure. Against a dev event hub:

1. Run a capture with a key-based connection string, then with
   `DeveloperTools` auth after `az login`, and confirm events arrive.
2. Change filters mid-run and confirm only later events are affected.
3. Run a `Between` window that lies in the past against a quiet hub and
   confirm the run completes by itself.
