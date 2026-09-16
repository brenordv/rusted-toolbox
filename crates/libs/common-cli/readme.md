# common-cli

Shared command-line machinery for the toolbox:

- `CommonToolArgs`: the common CLI flags every tool flattens in (log level and
  channels, header toggle, verbose mode), plus `app_boot_up` to initialize
  logging and print the runtime header.
- `CommonToolArgsNoVerbose`: the flatten shape for a tool that owns its own
  verbosity flag (a `-v` count, say) and derives its own default log level.
  No `--verbose`, and `--log-level` is optional: `app_boot_up_with_level`
  takes the tool's derived level and an explicit `--log-level` overrides it.
- `AppLogger`: the tracing bootstrap behind both boot paths.
- `tool_exit_helpers`: process exit helpers with consistent exit codes.
- `broken_pipe`: a `BrokenPipe` marker plus `write_out`/`flush_out` wrappers so a
  streaming tool can treat a consumer-closed pipe as a clean end of output
  (exit 0) instead of a write error.

The flag definitions live in the clap derives (see any tool's `--help`); the
per-item contracts live in this crate's rustdoc.

## Logging contract

- Diagnostics go to stderr by default, so stdout stays a clean data channel for
  piped consumers. `--log-to-console` opts into stdout with the timestamped
  format.
- The configured `--log-level` yields to `RUST_LOG` when that variable is set.
  The exception is `--log-level disabled`, which silences everything and
  overrides `RUST_LOG`.
- Under `CommonToolArgsNoVerbose`, the level a tool derives from its own flags
  is the weakest layer: an explicit `--log-level` beats it, and `RUST_LOG`
  beats both (with the same `disabled` exception).
- `--log-to-file` adds an append-mode file layer under the tool's `logs`
  subfolder in the user's home; `--rotate-log-file-by-day` puts the date in the
  filename. On Unix the logs folder is created 0o700 and the file is kept 0o600
  (a pre-existing wide file is tightened on open); on Windows the file inherits
  the user profile's ACL.