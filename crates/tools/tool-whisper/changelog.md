# Changelog

## 2.0.1
- The send and receive threads now report errors as `[system]` lines in the chat instead of dying silently. An oversized or failed outgoing message is dropped and the session keeps going, and the same holds for an incoming message that cannot be decrypted. A framing error on the incoming stream (an unreadable header or payload) is fatal for the receive side: the stream is desynchronized at that point, so the chat shows a `receiving stopped` notice and no further messages arrive.
- Outgoing messages are checked against the PKCS#1 v1.5 size limit before encryption. The limit is derived from the peer's key at handshake time (key size in bytes minus 11, about 501 bytes for the 4096-bit keys the tool generates); an oversized message is not sent and the UI says so.
- When the peer disconnects, the chat shows a `peer disconnected` notice and the receive thread stops cleanly.
- Incoming frames are capped at 64 KiB: a length header above the cap is rejected, naming the announced size, without allocating it.
- Debug logging no longer includes message content, so `--log-to-file` cannot persist the conversation; only sizes and counts are logged.
- `Connection` is now generic over any readable/writable stream (with a `TcpConnection` alias for the TCP path). Framing tests now run against an in-memory stream. Added tests for framing round-trips, the frame cap, the encryption size boundary, and the UI cursor math with multibyte characters.
- Thread join results are now logged instead of discarded.
- Removed the empty `encrypt_traits` module.
- Readme corrections: the message size limit is about 501 bytes for 4096-bit keys (was misstated as 446), the banner shows the current version, and the chat layout depiction matches the borderless message pane.
- Readme: the example startup lines are info-level log events (stderr, hidden at the default `warn` level), so the example commands now pass `--log-level info`; the shared common-cli flags are documented; the `No logging or message history` feature line now says message content is never logged.

## 2.0.0
- Refactored to fit the new tooling: CLI parsing now uses the `clap` derive API and the shared `CommonToolArgs`, replacing the hand-built `Command` from the retired `shared` crate.
- `--wait` can now be given without a value to listen on the default port (2428).
- Logging is now controlled at runtime through the shared flags (`--log-level`, `--log-to-file`, `--rotate-log-file-by-day`, `--log-to-console`) instead of compile-time toggles; use `--log-to-file` to capture a debug log without disturbing the chat UI.
- Gained the shared `--app-header` flag, which prints the runtime configuration before the session starts.

## 1.0.1
- Updated dependencies.

## 1.0.0
- Initial release.