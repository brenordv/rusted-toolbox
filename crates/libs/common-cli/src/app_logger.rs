use crate::tool_log_level::ToolLogLevel;
use common_utils::file_system::{get_app_sub_folder, get_filename_with_current_date};
use std::path::PathBuf;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};

/// A boxed tracing [`Layer`] over the global [`Registry`], suitable for handing
/// to an externally owned subscriber (such as the OpenTelemetry one).
pub type BoxedLayer = Box<dyn Layer<Registry> + Send + Sync + 'static>;

pub struct AppLogger {
    enabled: bool,
    to_console: bool,
    to_file: bool,
    rotate_log_file_by_day: bool,
    log_level: String,
    log_file_folder: PathBuf,
}

impl AppLogger {
    pub fn new(
        log_level: ToolLogLevel,
        to_console: bool,
        to_file: bool,
        rotate_log_file_by_day: bool,
    ) -> Self {
        Self {
            enabled: log_level != ToolLogLevel::Disabled,
            to_console,
            to_file,
            rotate_log_file_by_day,
            log_level: log_level.to_tracing_level(),
            log_file_folder: get_app_sub_folder("logs".to_string()),
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

    /// Builds the EnvFilter for this logger's own subscriber: RUST_LOG if set,
    /// else the configured level, else error.
    fn env_filter(&self) -> EnvFilter {
        EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new(self.tracing_level()))
            .unwrap_or_else(|_| EnvFilter::new(ToolLogLevel::Error.to_tracing_level()))
    }

    pub fn init(&self, force_disable_log: bool) {
        if !self.enabled && force_disable_log {
            return;
        }

        let env_filter = self.env_filter();

        let mut layers = Vec::new();

        if self.to_console {
            layers.push(tracing_subscriber::fmt::layer().boxed());
        }

        if self.to_file {
            let log_file = self.resolve_log_filename();

            if let Ok(file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_file)
            {
                let file_layer = tracing_subscriber::fmt::layer()
                    .with_writer(file)
                    .with_ansi(false) // ANSI in the log file would mess the log.
                    .boxed();
                layers.push(file_layer);
            }
        }

        let subscriber = tracing_subscriber::registry().with(env_filter).with(layers);

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
    /// append. Returns `None` when file logging is disabled or the file cannot
    /// be opened.
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
            .open(log_file)
        {
            Ok(file) => Some(
                tracing_subscriber::fmt::layer()
                    .with_writer(file)
                    .with_ansi(false)
                    .boxed(),
            ),
            Err(_) => None,
        }
    }

    fn resolve_log_filename(&self) -> PathBuf {
        let app_name = common_utils::app_name!();
        let filename = if self.rotate_log_file_by_day {
            get_filename_with_current_date(app_name.to_string(), "log".to_string(), false, true)
        } else {
            format!("{}.log", app_name)
        };

        let _ = std::fs::create_dir_all(&self.log_file_folder);

        self.log_file_folder.join(filename)
    }
}