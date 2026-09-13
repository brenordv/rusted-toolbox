use common_cli::app_logger::AppLogger;
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::tool_log_level::ToolLogLevel;

/// Opaque guard that keeps the OpenTelemetry providers alive.
///
/// Returned by [`OtelAppLogger::init_with_otel`] and
/// [`OtelCommonToolArgs::app_boot_up_with_otel`] when OpenTelemetry is active.
/// Hold it for the lifetime of the application; dropping it flushes and shuts
/// down the OTel providers. It cannot be constructed outside this module.
///
/// Without the `otel` feature, the guard is inert: it carries no provider and
/// dropping it does nothing. With the feature, the drop-before-exit-helpers rule
/// documented on [`OtelCommonToolArgs::app_boot_up_with_otel`] applies.
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
    /// `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable; a value that is empty
    /// after trimming counts as absent in either place.
    ///
    /// When the `otel` feature is enabled and an endpoint is resolved, traces and
    /// logs are exported to the collector and written to stdout by raccoon-otel.
    /// The `AppLogger` terminal-channel flags do not apply on this path, but
    /// `--log-to-file` still installs the file layer alongside the export. Returns
    /// `Some(OtelGuard)` — hold it for the application's lifetime.
    ///
    /// Otherwise (feature disabled, no endpoint, or OTel setup failed) this falls
    /// back to the standard [`AppLogger::init`] path — stderr or stdout, and/or a
    /// file, per the logger's own configuration — and returns `None`.
    ///
    /// `force_disable_log`, and a logger configured at `--log-level disabled`,
    /// both short-circuit before any endpoint resolution or OTel setup, install no
    /// subscriber, and return `None`.
    ///
    /// `service_name` is the OTel service identifier; callers typically pass their
    /// own `env!("CARGO_PKG_NAME")`.
    fn init_with_otel(
        &self,
        service_name: &str,
        otel_endpoint: Option<&str>,
        force_disable_log: bool,
    ) -> Option<OtelGuard>;
}

/// Trims `value` and returns it, treating an empty result as absent. Never logs
/// the value: OTLP endpoints can carry userinfo or token query parameters.
fn normalize_endpoint(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Resolves the OTLP endpoint from the explicit parameter, then from
/// `OTEL_EXPORTER_OTLP_ENDPOINT`. A value empty after trimming counts as absent
/// in both.
fn resolve_endpoint(param: Option<&str>) -> Option<String> {
    param.and_then(normalize_endpoint).or_else(|| {
        std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .ok()
            .as_deref()
            .and_then(normalize_endpoint)
    })
}

/// Reports whether an OTLP endpoint is configured: the explicit `otel_endpoint`
/// value first, falling back to `OTEL_EXPORTER_OTLP_ENDPOINT`; a value that is
/// empty after trimming counts as absent in either place, matching the rules
/// [`OtelAppLogger::init_with_otel`] applies.
///
/// Only presence is reported. The endpoint value itself is never returned or
/// logged: OTLP endpoints can carry userinfo or token query parameters.
pub fn is_otel_endpoint_configured(otel_endpoint: Option<&str>) -> bool {
    resolve_endpoint(otel_endpoint).is_some()
}

impl OtelAppLogger for AppLogger {
    fn init_with_otel(
        &self,
        service_name: &str,
        otel_endpoint: Option<&str>,
        force_disable_log: bool,
    ) -> Option<OtelGuard> {
        if force_disable_log || self.tracing_level().is_empty() {
            self.init(force_disable_log);
            return None;
        }

        let endpoint = resolve_endpoint(otel_endpoint);

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
                    Err(_) => {
                        // The raw error is deliberately not printed: raccoon-otel's
                        // setup errors can plausibly embed the OTLP endpoint, and
                        // this crate's contract is to never log that value.
                        eprintln!(
                            "Warning: failed to initialize OpenTelemetry; falling back to \
                             standard logging. The error is withheld because it may contain \
                             the OTLP endpoint."
                        );
                    }
                }
            }
        }

        #[cfg(not(feature = "otel"))]
        {
            if endpoint.is_some() {
                eprintln!(
                    "Warning: an OpenTelemetry endpoint was provided but this binary was \
                     built without the `otel` feature; telemetry is disabled and standard \
                     logging is used."
                );
            }
            let _ = service_name;
        }

        self.init(force_disable_log);
        None
    }
}

/// Extends [`CommonToolArgs`] with a one-call OpenTelemetry boot path.
pub trait OtelCommonToolArgs {
    /// Boots the tool like [`CommonToolArgs::app_boot_up`] (the header and
    /// runtime-config display are honored), then initializes logging with
    /// OpenTelemetry when the `otel` feature and a resolved endpoint are both
    /// present. The tool's package name (`app_name`) is used as both the log
    /// identity and the OTel `service_name`.
    ///
    /// Returns the [`OtelGuard`] when the OTel path is active, otherwise `None`.
    ///
    /// # Contract
    /// - Hold the returned guard for the whole `main`; telemetry flushes when it
    ///   drops.
    /// - The workspace's exit helpers (`exit_success`, `exit_error`,
    ///   `exit_with_code`) terminate the process without running `Drop`, so a tool
    ///   that ends through them with the guard still alive loses its buffered
    ///   telemetry. Drop the guard explicitly (`drop(guard)`) or let it fall out of
    ///   scope before calling any exit helper.
    /// - Without the `otel` feature or an endpoint the tool gets standard
    ///   [`AppLogger`] behavior.
    /// - On the OTel path the console layer comes from raccoon-otel (stdout) and
    ///   the `AppLogger` terminal-channel flags do not apply, while `--log-to-file`
    ///   still installs the file layer.
    /// - `--log-level disabled` and `force_disable_log` both suppress everything,
    ///   OpenTelemetry included.
    fn app_boot_up_with_otel(
        &self,
        app_name: &str,
        app_version: &str,
        uses_verbose_flag: bool,
        force_disable_log: bool,
        otel_endpoint: Option<&str>,
        tool_header_printer: Option<impl FnOnce()>,
    ) -> Option<OtelGuard>;
}

impl OtelCommonToolArgs for CommonToolArgs {
    fn app_boot_up_with_otel(
        &self,
        app_name: &str,
        app_version: &str,
        uses_verbose_flag: bool,
        force_disable_log: bool,
        otel_endpoint: Option<&str>,
        tool_header_printer: Option<impl FnOnce()>,
    ) -> Option<OtelGuard> {
        let disable = force_disable_log || self.default_logging_level == ToolLogLevel::Disabled;

        self.app_boot_up(
            app_name,
            app_version,
            uses_verbose_flag,
            true,
            tool_header_printer,
        );

        self.build_logger(app_name)
            .init_with_otel(app_name, otel_endpoint, disable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The enabled-`otel` export path needs a running collector and the global
    // subscriber slot, so it is covered only at compile level (via
    // `cargo check --features otel`), never here.

    #[test]
    fn resolve_endpoint_normalizes_and_falls_back() {
        // OTEL_EXPORTER_OTLP_ENDPOINT is process-global and tests run in parallel;
        // this is the only test that touches it, and it restores the original.
        const VAR: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
        let original = std::env::var(VAR).ok();

        std::env::remove_var(VAR);
        assert_eq!(
            resolve_endpoint(Some("http://collector:4318")),
            Some("http://collector:4318".to_string())
        );
        assert_eq!(
            resolve_endpoint(Some("  http://collector:4318  ")),
            Some("http://collector:4318".to_string())
        );
        assert_eq!(resolve_endpoint(Some("")), None);
        assert_eq!(resolve_endpoint(Some("   ")), None);
        assert_eq!(resolve_endpoint(None), None);
        assert!(is_otel_endpoint_configured(Some("http://collector:4318")));
        assert!(!is_otel_endpoint_configured(Some("   ")));
        assert!(!is_otel_endpoint_configured(None));

        std::env::set_var(VAR, "http://from-env:4318");
        assert_eq!(
            resolve_endpoint(None),
            Some("http://from-env:4318".to_string())
        );
        assert_eq!(
            resolve_endpoint(Some("http://param:4318")),
            Some("http://param:4318".to_string())
        );
        assert!(is_otel_endpoint_configured(None));

        std::env::set_var(VAR, "   ");
        assert_eq!(resolve_endpoint(None), None);
        assert!(!is_otel_endpoint_configured(None));

        match original {
            Some(value) => std::env::set_var(VAR, value),
            None => std::env::remove_var(VAR),
        }
    }

    #[test]
    fn init_with_otel_force_disabled_returns_none() {
        let logger = AppLogger::new("logging-otel-test", ToolLogLevel::Info, false, false, false);

        let guard = logger.init_with_otel("logging-otel-test", Some("http://collector:4318"), true);

        assert!(guard.is_none());
    }

    #[test]
    fn init_with_otel_disabled_level_returns_none() {
        // Asserts only on the return value; probing the global dispatcher would
        // couple this test to run order. The Disabled level short-circuits
        // before endpoint resolution, so the endpoint here is never read.
        let logger = AppLogger::new(
            "logging-otel-test",
            ToolLogLevel::Disabled,
            false,
            false,
            false,
        );

        let guard =
            logger.init_with_otel("logging-otel-test", Some("http://collector:4318"), false);

        assert!(guard.is_none());
    }
}
