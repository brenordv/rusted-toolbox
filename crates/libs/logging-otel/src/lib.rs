//! OpenTelemetry extension for the toolbox's logging (`common-cli`).
//!
//! Adds two extension traits: [` OtelAppLogger `] on `AppLogger`, and
//! [` OtelCommonToolArgs `] on `CommonToolArgs` for a one-call boot path. Both
//! return an [` OtelGuard `] that must be held for the process lifetime and dropped
//! before any exit helper, since the exit helpers skip `Drop` and would lose
//! buffered telemetry.
//!
//! # Features
//! - `default` (no features): the OTel code is compiled out; `init_with_otel` and
//!   `app_boot_up_with_otel` fall back to standard `AppLogger` behavior, and the
//!   returned guard is inert.
//! - `otel`: pulls in `raccoon-otel` and performs real OTLP export when an
//!   endpoint is resolved.

pub mod otel_app_logger;

pub use otel_app_logger::{
    is_otel_endpoint_configured, OtelAppLogger, OtelCommonToolArgs, OtelGuard,
};
