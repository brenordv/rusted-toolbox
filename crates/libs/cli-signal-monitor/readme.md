# cli-signal-monitor

Ctrl-C (SIGINT) handling for the toolbox's CLI tools, in one call:

```rust
use std::sync::atomic::Ordering;
use cli_signal_monitor::setup_graceful_shutdown::setup_graceful_shutdown;

let shutdown = setup_graceful_shutdown(false);
while !shutdown.load(Ordering::Relaxed) {
    // do a unit of work, then re-check the flag
}
```

## Contract

- With `immediate_exit` false (every current caller), the returned `AtomicBool`
  starts `false` and flips to `true` on interrupt. The tool polls it and winds
  down its own work; nothing exits on its behalf.
- With `immediate_exit` true, the handler exits the process itself with code 0,
  via `exit_success()`. Two consequences, both intentional while the mode has no
  callers: a script cannot tell an interrupt from a normal success by exit code
  (`common_utils::constants::EXIT_CODE_INTERRUPTED_BY_USER` exists if that ever
  needs to change), and `Drop` implementations do not run, so buffered state
  such as a `logging-otel` `OtelGuard`'s telemetry is lost.
- The handler installs once per process. A second call fails to register, and
  the process exits via `exit_error`, reporting the failure through the tool's
  logging.

The shutdown messages are emitted through `tracing`, so they land on stderr
under the default logging configuration (stdout under `--log-to-console`), and
nowhere when logging is disabled or no subscriber is installed yet.
