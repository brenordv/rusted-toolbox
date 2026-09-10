# Changelog

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