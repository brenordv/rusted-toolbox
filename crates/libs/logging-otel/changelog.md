# Changelog

## 1.1.0
- Endpoint resolution now normalizes whitespace and treats an empty value as
  absent, for both the parameter and `OTEL_EXPORTER_OTLP_ENDPOINT`.
- Building without the `otel` feature while an endpoint is provided now prints a
  warning instead of silently doing nothing.
- `force_disable_log` and `--log-level disabled` are honored on the
  OpenTelemetry path, short-circuiting before any endpoint resolution or export
  setup.
- Added `OtelCommonToolArgs::app_boot_up_with_otel`, a one-call boot seam over
  `CommonToolArgs` that keeps the header display and reuses the tool's package
  name as both log identity and OTel service name.
- Documented the guard lifetime and the rule that the guard must drop before any
  exit helper, since the exit helpers skip `Drop`.
- Corrected the trait docs that claimed output is always mirrored to stdout.
- Added the first unit tests.

## 1.0.0
- Version bump from 0.1.0 with a crate description added; no functional change.

## 0.1.0
- Crate created in the workspace split: `OtelGuard`, the `OtelAppLogger`
  extension trait, and the `otel` feature gating `raccoon-otel`.
