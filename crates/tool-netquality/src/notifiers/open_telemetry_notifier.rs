use anyhow::{Context, Result};
use opentelemetry::global;
use opentelemetry::trace::{Span as _, Tracer as _};
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing::warn;

pub(crate) struct OpenTelemetryNotifier {
    tracer: global::BoxedTracer,
    provider: SdkTracerProvider,
}

impl OpenTelemetryNotifier {
    pub(crate) fn new(endpoint: &str) -> Result<Self> {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(endpoint)
            .build()
            .context("Failed to build OpenTelemetry exporter")?;

        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter)
            .build();

        global::set_tracer_provider(provider.clone());

        let tracer = global::tracer("netquality");

        Ok(Self { tracer, provider })
    }

    pub(crate) fn send(&self, message: &str) -> Result<()> {
        let mut span = self.tracer.start("netquality.notification");
        span.set_attribute(KeyValue::new("message", message.to_string()));
        span.end();
        Ok(())
    }

    pub(crate) fn shutdown(&self) {
        if let Err(error) = self.provider.shutdown() {
            warn!("Failed to shutdown OpenTelemetry provider: {error}");
        }
    }
}
