# common-cli

Shared command-line machinery for the toolbox:

- `CommonToolArgs`: the common CLI flags every tool flattens in (log level and
  channels, header toggle, verbose mode), plus `app_boot_up` to initialize
  logging and print the runtime header.
- `AppLogger`: the tracing bootstrap behind `app_boot_up`.
- `tool_exit_helpers`: process exit helpers with consistent exit codes.

The flag definitions live in the clap derives (see any tool's `--help`); the
per-item contracts live in this crate's rustdoc.