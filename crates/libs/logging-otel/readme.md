# logging-otel

OpenTelemetry extension for `common-cli`'s logging. Two extension traits:
`OtelAppLogger::init_with_otel` on `AppLogger`, and
`OtelCommonToolArgs::app_boot_up_with_otel` on `CommonToolArgs` for a one-call
boot path.

## Feature gate

The `default` build compiles the OTel code out entirely: both entry points fall
back to standard `AppLogger` behavior and the returned guard is inert. Build
with `--features otel` to pull in `raccoon-otel` and export for real. A binary
built without the feature that still receives an endpoint prints a warning and
uses standard logging.

## Endpoint resolution

The OTLP endpoint comes from the explicit parameter first, then the
`OTEL_EXPORTER_OTLP_ENDPOINT` environment variable; a value that is empty after
trimming counts as absent in either place. The endpoint value is never logged,
including on setup failure: OTLP endpoints can carry userinfo or token query
parameters.

`is_otel_endpoint_configured(Option<&str>) -> bool` exposes the same resolution
as a presence check, for callers that only report whether export is configured
(for example a runtime-info header line). It returns a bool and nothing else,
so the never-log-the-endpoint contract holds by construction.

## Adoption recipe (a tool boots with OTel)

```rust,ignore
let args = WhateverArgs::parse();
let guard = args.common.app_boot_up_with_otel(
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_VERSION"),
    true,          // tool uses --verbose
    false,         // force_disable_log
    None,          // endpoint: fall back to OTEL_EXPORTER_OTLP_ENDPOINT
    None::<fn()>,  // tool_header_printer
);
// hold `guard` for all of main; telemetry flushes when it drops
```

## Contracts worth knowing

- Hold the returned `OtelGuard` for the whole `main`. The workspace exit
  helpers (`exit_success`, `exit_error`, `exit_with_code`) skip `Drop`, so
  ending through them with the guard alive loses buffered telemetry; drop the
  guard first.
- On the OTel path, console output comes from raccoon-otel and goes to stdout,
  so the stderr-default "stdout is a clean data channel" contract from
  `common-cli` does not hold while an endpoint is active. `--log-to-file` still
  installs the file layer alongside the export.
- The OTel path seeds `RUST_LOG` from the configured level when the variable is
  unset. That mutation is process-wide and inherited by any child process the
  tool spawns.
- `--log-level disabled` (and the force-disable flag) suppress everything,
  OpenTelemetry included, before any endpoint resolution happens.
- Setup failure warns on stderr (without the error detail, which may contain
  the endpoint) and degrades to standard logging.
