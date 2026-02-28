use crate::logging::app_logger::{AppLogger, LogLevel};

/// Opaque guard that keeps OTel providers alive while held.
///
/// When the `otel` feature is enabled, this wraps [`raccoon_otel::OtelGuard`].
/// When the feature is disabled, `initialize_log_with_otel` always returns `None`.
///
/// This type cannot be constructed outside of this module.
pub struct OtelGuard {
    #[cfg(feature = "otel")]
    _inner: raccoon_otel::OtelGuard,
    /// Private field to prevent external construction via struct literal.
    _private: (),
}

/// Initializes the logging system for the application with the specified settings.
///
/// This function sets up a logger for the application using the provided application name and log level.
/// The logger is configured to be enabled, logs to the console, and assigns the given application name.
///
/// # Parameters
///
/// * `app_name` - A string slice that holds the name of the application.
/// * `log_level` - A `LogLevel` value specifying the logging verbosity level for the application.
///
/// This example initializes the logger for an application named "MyApp" with an information log level.
///
/// # Notes
///
/// Ensure that the logging system is initialized before attempting to log messages in the application,
/// as failure to do so may result in logs not being recorded.
///
/// # Panics
///
/// This function may panic if the logging system fails to initialize or if there are issues with the provided configuration.
pub fn initialize_log(app_name: &str, log_level: LogLevel) {
    get_default_log_builder(app_name, log_level).init();
}

/// Initializes logging with optional OpenTelemetry support.
///
/// Resolves the OTel endpoint from the explicit parameter first, then falls back to
/// the `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable. When an endpoint is resolved
/// and the `otel` feature is enabled, delegates to [`raccoon_otel::setup_otel`] which
/// sets up the global tracing subscriber with fmt + OTel layers.
///
/// Otherwise, falls through to the standard [`initialize_log`] path.
///
/// # Returns
///
/// `Some(OtelGuard)` when OTel was successfully initialized — **hold this guard for the
/// lifetime of the application** so providers flush on drop.
/// `None` when OTel is not configured or the feature is disabled.
pub fn initialize_log_with_otel(
    app_name: &str,
    log_level: LogLevel,
    otel_endpoint: Option<&str>,
) -> Option<OtelGuard> {
    let endpoint = otel_endpoint
        .map(String::from)
        .or_else(|| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok());

    #[cfg(feature = "otel")]
    {
        if let Some(ep) = endpoint {
            use std::time::Duration;

            // Set RUST_LOG so raccoon_otel's EnvFilter picks up the requested level.
            // Only set it if the user hasn't already configured RUST_LOG explicitly.
            if std::env::var("RUST_LOG").is_err() {
                std::env::set_var("RUST_LOG", log_level.to_tracing_level());
            }

            match raccoon_otel::setup_otel(
                app_name,
                Some(
                    raccoon_otel::OtelOptions::builder()
                        .endpoint(&ep)
                        .protocol(raccoon_otel::Protocol::HttpProtobuf)
                        .export_timeout(Duration::from_secs(30))
                        .build(),
                ),
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
        let _ = endpoint; // suppress unused warning
    }

    // Fall back to standard logging
    initialize_log(app_name, log_level);
    None
}

pub fn get_default_log_builder(app_name: &str, log_level: LogLevel) -> AppLogger {
    let mut builder = AppLogger::new(true);

    builder
        .log_level(log_level)
        .app_name(app_name)
        .log_to_console(true);

    builder
}
