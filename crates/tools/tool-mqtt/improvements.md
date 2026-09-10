# Basic
[X] Migrate cli to the new pattern.
[X] Add more test coverage to the tool. Outcome (2.1.0): added tests for the four CLI validators, the clap definition (`debug_assert`), the `reads`/`send` aliases, and the password-only `is_anonymous` edge.
[X] Research improvements to the tool. Outcome (2.1.0): the subscriber died on the first non-UTF-8 payload, fixed with lossy conversion plus escaped display; the readme advertised `reads`/`send` aliases that did not exist, added; the subcommand help shipped `<ADD EXAMPLE>` placeholders, filled with real examples.
[ ] Adopt cli-signal-monitor graceful shutdown so the read/post loops exit cleanly on Ctrl+C instead of running until killed.
[X] Print received messages to stdout instead of (or in addition to) `info!` logging: with the default warn log level a subscriber shows nothing when messages arrive, which reads as a hang. The readme documents the `--log-level info` workaround today. Outcome (2.2.0): payloads print to stdout one line per message with control and bidi-control characters escaped; broken-pipe exits cleanly; readme workaround removed.
[ ] Dedupe or back off the subscriber's connection error logging: a dead broker currently emits an identical `error!` every ~200ms forever.
