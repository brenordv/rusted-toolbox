use crate::tool_log_level::ToolLogLevel;
use common_utils::file_system::{get_app_sub_folder, get_filename_with_current_date};
use std::io::IsTerminal;
use std::path::PathBuf;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};

/// A boxed tracing [`Layer`] over the global [`Registry`], suitable for handing
/// to an externally owned subscriber (such as the OpenTelemetry one).
pub type BoxedLayer = Box<dyn Layer<Registry> + Send + Sync + 'static>;

/// Which terminal stream the always-present terminal layer writes to.
#[derive(Debug, PartialEq)]
enum TerminalTarget {
    /// stdout, selected by `--log-to-console`; keeps the timestamped format.
    Stdout,
    /// stderr, the default, so a tool that only calls `error!` still shows its
    /// fatal output while stdout stays a clean data channel.
    Stderr,
}

/// The layers to install, derived purely from [`AppLogger`] state.
///
/// A `Some` plan always has a terminal layer; `file` adds the file layer on top.
/// [`AppLogger::plan`] returns `None` to mean "install no subscriber at all".
#[derive(Debug, PartialEq)]
struct LayerPlan {
    terminal: TerminalTarget,
    file: bool,
    level: String,
}

pub struct AppLogger {
    app_name: String,
    enabled: bool,
    to_console: bool,
    to_file: bool,
    rotate_log_file_by_day: bool,
    log_level: String,
    log_file_folder: PathBuf,
}

impl AppLogger {
    pub fn new(
        app_name: &str,
        log_level: ToolLogLevel,
        to_console: bool,
        to_file: bool,
        rotate_log_file_by_day: bool,
    ) -> Self {
        Self {
            app_name: app_name.to_string(),
            enabled: log_level != ToolLogLevel::Disabled,
            to_console,
            to_file,
            rotate_log_file_by_day,
            log_level: log_level.to_tracing_level(),
            log_file_folder: get_app_sub_folder(app_name, "logs".to_string()),
        }
    }

    /// Seeds `RUST_LOG` from this logger's configured level unless the user
    /// already set it, so a subscriber that reads its filter from the
    /// environment (e.g. the OTel one) honors the configured level.
    pub fn seed_rust_log(&self) {
        if std::env::var("RUST_LOG").is_err() {
            std::env::set_var("RUST_LOG", self.tracing_level());
        }
    }

    /// Builds the EnvFilter for this logger's own subscriber: `RUST_LOG` if set,
    /// else the configured `level`, else error.
    fn env_filter(level: &str) -> EnvFilter {
        EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new(level))
            .unwrap_or_else(|_| EnvFilter::new(ToolLogLevel::Error.to_tracing_level()))
    }

    /// Decides which layers to install from this logger's configuration.
    ///
    /// Returns `None` when logging is off (the caller force-disabled it, or the
    /// level is [`ToolLogLevel::Disabled`]), so [`init`](Self::init) installs no
    /// subscriber. The `level` carried here is the configured level only;
    /// `RUST_LOG` precedence is applied later in [`env_filter`](Self::env_filter),
    /// which keeps this decision free of environment reads and therefore testable.
    fn plan(&self, force_disable: bool) -> Option<LayerPlan> {
        if force_disable || !self.enabled {
            return None;
        }

        let terminal = if self.to_console {
            TerminalTarget::Stdout
        } else {
            TerminalTarget::Stderr
        };

        Some(LayerPlan {
            terminal,
            file: self.to_file,
            level: self.log_level.clone(),
        })
    }

    /// Builds the terminal layer for the chosen stream. stdout keeps the default
    /// timestamped format; stderr drops the timestamp and gates ANSI on whether
    /// stderr is a TTY, so redirected output carries no escape codes.
    fn terminal_layer(target: &TerminalTarget) -> BoxedLayer {
        match target {
            TerminalTarget::Stdout => tracing_subscriber::fmt::layer().boxed(),
            TerminalTarget::Stderr => tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .without_time()
                .with_ansi(std::io::stderr().is_terminal())
                .boxed(),
        }
    }

    pub fn init(&self, force_disable_log: bool) {
        let Some(plan) = self.plan(force_disable_log) else {
            return;
        };

        let mut layers: Vec<BoxedLayer> = vec![Self::terminal_layer(&plan.terminal)];

        if plan.file {
            if let Some(file_layer) = self.file_layer() {
                layers.push(file_layer);
            }
        }

        let subscriber = tracing_subscriber::registry()
            .with(layers)
            .with(Self::env_filter(&plan.level));

        subscriber.init();
    }

    /// The tracing level string this logger was configured with (e.g. `"info"`).
    ///
    /// Useful for seeding `RUST_LOG` before handing control to a subscriber that
    /// reads its filter from the environment.
    pub fn tracing_level(&self) -> &str {
        &self.log_level
    }

    /// Builds the file-writing layer when this logger is configured to log to a
    /// file, resolving the (optionally date-rotated) path and opening it for
    /// append. Returns `None` when file logging is disabled, or when the file
    /// cannot be opened. On open failure a single warning naming the path and the
    /// error is written to stderr, since the logging system cannot use `tracing`
    /// to report its own bootstrap failure.
    ///
    /// The layer is typed over the global [`Registry`] so it can be added to a
    /// subscriber owned elsewhere (for example the OpenTelemetry one) rather
    /// than only the one built by [`AppLogger::init`].
    pub fn file_layer(&self) -> Option<BoxedLayer> {
        if !self.to_file {
            return None;
        }

        let log_file = self.resolve_log_filename();

        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
        {
            Ok(file) => Some(
                tracing_subscriber::fmt::layer()
                    .with_writer(file)
                    .with_ansi(false)
                    .boxed(),
            ),
            Err(error) => {
                eprintln!(
                    "Warning: could not open log file {}: {error}",
                    log_file.display()
                );
                None
            }
        }
    }

    fn resolve_log_filename(&self) -> PathBuf {
        let filename = if self.rotate_log_file_by_day {
            get_filename_with_current_date(self.app_name.to_string(), "log".to_string(), false, true)
        } else {
            format!("{}.log", self.app_name)
        };

        let _ = std::fs::create_dir_all(&self.log_file_folder);

        self.log_file_folder.join(filename)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logger(level: ToolLogLevel, to_console: bool, to_file: bool) -> AppLogger {
        AppLogger::new(env!("CARGO_PKG_NAME"), level, to_console, to_file, false)
    }

    #[test]
    fn plan_defaults_to_stderr_at_warn() {
        let plan = logger(ToolLogLevel::Warn, false, false).plan(false);
        assert_eq!(
            plan,
            Some(LayerPlan {
                terminal: TerminalTarget::Stderr,
                file: false,
                level: "warn".to_string(),
            })
        );
    }

    #[test]
    fn plan_log_to_console_selects_stdout() {
        let plan = logger(ToolLogLevel::Warn, true, false).plan(false).unwrap();
        assert_eq!(plan.terminal, TerminalTarget::Stdout);
        assert!(!plan.file);
    }

    #[test]
    fn plan_log_to_file_keeps_stderr_and_adds_file() {
        let plan = logger(ToolLogLevel::Warn, false, true).plan(false).unwrap();
        assert_eq!(plan.terminal, TerminalTarget::Stderr);
        assert!(plan.file);
    }

    #[test]
    fn plan_both_channels_select_stdout_and_file() {
        let plan = logger(ToolLogLevel::Warn, true, true).plan(false).unwrap();
        assert_eq!(plan.terminal, TerminalTarget::Stdout);
        assert!(plan.file);
    }

    #[test]
    fn plan_disabled_level_installs_nothing() {
        assert_eq!(
            logger(ToolLogLevel::Disabled, false, false).plan(false),
            None
        );
    }

    #[test]
    fn plan_disabled_level_beats_console_flag() {
        assert_eq!(
            logger(ToolLogLevel::Disabled, true, false).plan(false),
            None
        );
    }

    #[test]
    fn plan_disabled_level_beats_file_flag() {
        assert_eq!(
            logger(ToolLogLevel::Disabled, false, true).plan(false),
            None
        );
    }

    #[test]
    fn plan_force_disable_overrides_enabled_logger() {
        assert_eq!(logger(ToolLogLevel::Info, true, true).plan(true), None);
    }

    #[test]
    fn plan_carries_configured_level() {
        assert_eq!(
            logger(ToolLogLevel::Debug, false, false)
                .plan(false)
                .unwrap()
                .level,
            "debug"
        );
        assert_eq!(
            logger(ToolLogLevel::Error, false, false)
                .plan(false)
                .unwrap()
                .level,
            "error"
        );
    }
}
