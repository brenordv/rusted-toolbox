use common_cli::app_logger::AppLogger;

/// Opaque guard that keeps the OpenTelemetry providers alive.
///
/// Returned by [`OtelAppLogger::init_with_otel`] when OpenTelemetry is active.
/// Hold it for the lifetime of the application; dropping it flushes and shuts
/// down the OTel providers. It cannot be constructed outside this module.
#[must_use = "dropping the OtelGuard shuts down OpenTelemetry — hold it for the application's lifetime"]
pub struct OtelGuard {
    #[cfg(feature = "otel")]
    _inner: raccoon_otel::OtelGuard,
    /// Private field to prevent external construction via struct literal.
    _private: (),
}

/// Extends [`AppLogger`] with OpenTelemetry-aware initialization.
///
/// This is the extension-trait pattern: the trait lives here in `logging-otel`
/// and is implemented for the `AppLogger` defined in `common-cli`, so the OTel
/// behavior is added without modifying `common-cli` (Rust forbids inherent
/// `impl` blocks on a type from another crate).
pub trait OtelAppLogger {
    /// Initializes logging, exporting to OpenTelemetry when an endpoint is available.
    ///
    /// The endpoint is resolved from `otel_endpoint` first, falling back to the
    /// `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable.
    ///
    /// When the `otel` feature is enabled and an endpoint is resolved, traces and
    /// logs are exported to the collector and mirrored to stdout; if this logger
    /// is configured to log to a file, that file layer is installed alongside
    /// them. Returns `Some(OtelGuard)` — hold it for the application's lifetime.
    ///
    /// Otherwise (feature disabled, no endpoint, or OTel setup failed) this falls
    /// back to the standard [`AppLogger::init`] path — console and/or file per the
    /// logger's own configuration — and returns `None`.
    ///
    /// `service_name` is the OTel service identifier; callers typically pass
    /// their own `env!("CARGO_PKG_NAME")`.
    fn init_with_otel(
        &self,
        service_name: &str,
        otel_endpoint: Option<&str>,
        force_disable_log: bool,
    ) -> Option<OtelGuard>;
}

impl OtelAppLogger for AppLogger {
    fn init_with_otel(
        &self,
        service_name: &str,
        otel_endpoint: Option<&str>,
        force_disable_log: bool,
    ) -> Option<OtelGuard> {
        let endpoint = otel_endpoint
            .map(String::from)
            .or_else(|| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok());

        #[cfg(feature = "otel")]
        {
            if let Some(endpoint) = endpoint {
                use std::time::Duration;

                self.seed_rust_log();

                // stdout comes from raccoon-otel itself; we only add the file layer.
                let extra_layers = self.file_layer().into_iter().collect();

                let options = raccoon_otel::OtelOptions::builder()
                    .endpoint(&endpoint)
                    .protocol(raccoon_otel::Protocol::HttpProtobuf)
                    .export_timeout(Duration::from_secs(30))
                    .build();

                match raccoon_otel::setup_otel_with_layers(
                    service_name,
                    Some(options),
                    extra_layers,
                ) {
                    Ok(guard) => {
                        return Some(OtelGuard {
                            _inner: guard,
                            _private: (),
                        })
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to initialize OpenTelemetry: {e}");
                    }
                }
            }
        }

        #[cfg(not(feature = "otel"))]
        {
            let _ = (&endpoint, service_name);
        }

        self.init(force_disable_log);
        None
    }
}
