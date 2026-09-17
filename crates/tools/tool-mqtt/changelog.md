# Changelog

## 2.3.1
- Broker credentials travel as a typed pair (`MqttCredentials`) built during argument
  resolution, so `create_connection_options` no longer unwraps `--username`/`--password`
  under a validation performed far away. A one-sided pair is unrepresentable past parsing
  and is rejected right after logging boots, with the same message as before. The struct
  deliberately has no `Debug` derive, so the password cannot leak through `{:?}` formatting.
  Cosmetic side effect on failing runs only: with a one-sided pair, `--app-header` now shows
  `Connection type: Anonymous` before the error (it used to show `Authenticated`).
- main follows the fleet's entrypoint pattern: argument and run failures log the full error
  chain through `error!` and exit 1 via the shared exit helpers, instead of returning
  `anyhow::Result` and getting anyhow's `Error: ...` debug print.

## 2.3.0
- Ctrl+C now stops both modes cleanly through the shared cli-signal-monitor flag instead of
  requiring a kill. The subscriber exits 0 (interrupting a subscription is its normal ending);
  the publisher exits with an error when interrupted before the broker acknowledged the
  publication, so a script can tell an unconfirmed publish from a confirmed one. During a
  backoff wait the flag is checked every 200ms.
- A dead broker no longer emits an identical `error!` line every ~200ms forever. Failed polls
  retry with exponential backoff (500ms doubling to a 30s cap, reset on recovery), the first
  occurrence of an error logs at error level, identical repeats drop to debug, and a recovery
  logs one info line.
- The flat per-event sleep is gone from both loops (200ms in the subscriber, 100ms in the
  publisher). It capped the subscriber at roughly five events per second on a busy topic;
  `EventLoop::poll` already blocks until the next event, so the loops now keep up with the
  broker and only the error path waits (with the backoff above).
- Payload escaping moved to `common_utils::string_utils::escape_for_terminal_display` (shared
  with pingx); rendered bytes are unchanged.

## 2.2.0
- Received messages now print to stdout, one line per message, instead of being reported through
  the logger at info level. A subscriber shows messages at the default warn log level, and stdout
  can be piped while logs stay on stderr. Note: `--log-to-console` routes logs to stdout and will
  interleave them with payloads; keep the default stderr logging when piping.
- Payload rendering escapes control characters (C0, DEL, C1) and Unicode bidirectional-control
  characters (overrides, isolates, and the implicit marks LRM/RLM/ALM); all other text passes
  through unchanged, so a single-line JSON payload pipes into `jq` as is. A multi-line payload is
  escaped onto one line. The previous rendering escaped quotes and backslashes too and wrapped the
  payload in quotes.
- A closed stdout pipe (for example piping into `head`) ends the subscription cleanly instead of
  panicking.

## 2.1.0
- Added the subcommand aliases the readme documents: `reads` for `read` and `send` for `post`,
  shown in the help output.
- A non-UTF-8 payload no longer kills the subscriber loop. The payload is lossy-converted for
  display (invalid bytes become U+FFFD), a warning is logged, and reading continues. The escaped
  rendering is kept, so control characters in broker-controlled bytes still cannot reach the
  terminal unescaped.
- The `read` and `post` help screens show real usage examples instead of the `<ADD EXAMPLE>`
  placeholder.
- Added unit tests for the CLI validators, the clap definition, the subcommand aliases, and the
  password-only `is_anonymous` edge.
- Runtime-config lines are rendered through `common-cli`'s `header_format` helpers; output bytes
  are unchanged. The now-unused direct `common-utils` dependency was dropped.

## 2.0.0
- Refactored to fit the new tooling.

## 1.0.2
- Updated dependencies.
- Changed config of `rumqttc` to skip using AWS_LC, and resort to `use-native-tls`. Want to keep things as simple as possible.

## 1.0.1
- Removed emojis. They don't render properly on every terminal.

## 1.0.0
Initial release