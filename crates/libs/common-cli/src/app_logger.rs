use crate::tool_log_level::ToolLogLevel;
use common_utils::file_system::{get_app_sub_folder, get_filename_with_current_date};
use std::io::IsTerminal;
use std::path::PathBuf;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};

/// A boxed tracing [` Layer `] over the global [` Registry `], suitable for handing
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

/// The layers to install, derived purely from the [` AppLogger `] state.
///
/// A `Some` plan always has a terminal layer; `file` adds the file layer on top.
/// [` AppLogger::plan `] returns `None` to mean "install no subscriber at all".
#[derive(Debug, PartialEq)]
struct LayerPlan {
    terminal: TerminalTarget,
    file: bool,
    level: String,
}

/// Owns a tool's logging configuration and builds the tracing layers from it.
///
/// Constructed from the parsed CLI flags via [` AppLogger::new `]. [` init `](Self::init)
/// installs the global subscriber, while [` file_layer `](Self::file_layer) and
/// [` seed_rust_log `](Self::seed_rust_log) let an externally owned subscriber reuse
/// this configuration.
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
    /// Builds a logger for `app_name` from the given level and output flags.
    ///
    /// The log file folder is the tool's `logs` subfolder under the user's home
    /// (`~/.<app_name>/logs`). `app_name` must be a plain name with no path
    /// separators; every caller passes its own `env!("CARGO_PKG_NAME")`.
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
            log_file_folder: get_app_sub_folder(app_name, "logs"),
        }
    }

    /// Seeds `RUST_LOG` from this logger's configured level unless the user
    /// already set it, so a subscriber that reads its filter from the
    /// environment (e.g. the OTel one) honors the configured level.
    pub fn seed_rust_log(&self) {
        if std::env::var("RUST_LOG").is_err() {
            // FIXME: Audit that the environment access only happens in single-threaded code.
            unsafe { std::env::set_var("RUST_LOG", self.tracing_level()) };
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

    /// Installs the global tracing subscriber for this logger.
    ///
    /// Does nothing when logging is off (`force_disable_log`, or a
    /// [`ToolLogLevel::Disabled`] level). The terminal layer is always present; the
    /// file layer is added when `--log-to-file` is set and the file can be opened.
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
    /// file, creating the log directory, resolving the (optionally date-rotated)
    /// path, and opening it for appending. Returns `None` when file logging is
    /// disabled, when the log directory cannot be created, or when the file cannot
    /// be opened (which on Unix includes a file whose permissions could not be
    /// restricted). Each failure writes a single warning naming the path and the
    /// error to stderr, since the logging system cannot use `tracing` to report
    /// its own bootstrap failure.
    ///
    /// The layer is typed over the global [`Registry`] so it can be added to a
    /// subscriber owned elsewhere (for example, the OpenTelemetry one) rather
    /// than only the one built by [`AppLogger::init`].
    pub fn file_layer(&self) -> Option<BoxedLayer> {
        if !self.to_file {
            return None;
        }

        if let Err(error) = create_log_dir(&self.log_file_folder) {
            eprintln!(
                "Warning: could not create log directory {}: {error}",
                self.log_file_folder.display()
            );
            return None;
        }

        let log_file = self.resolve_log_filename();

        match open_log_file(&log_file) {
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
            get_filename_with_current_date(&self.app_name, "log", false, true)
        } else {
            format!("{}.log", self.app_name)
        };

        self.log_file_folder.join(filename)
    }
}

/// Creates the log directory and any missing parents. On Unix every directory
/// it creates is owner-only (0o700), because log files can carry sensitive
/// content (whurl execution logs include response bodies); a directory that
/// already exists keeps its permissions.
#[cfg(unix)]
fn create_log_dir(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

/// Creates the log directory and any missing parents. Windows has no mode
/// bits; the directory inherits the parent's ACL (the user profile).
#[cfg(not(unix))]
fn create_log_dir(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Opens the log file for append, creating it owner-read/write (0o600). A file
/// that already exists is tightened to the same mode through the open handle
/// (fchmod semantics: no path re-resolution), so a log created wide by an older
/// version is healed on the next run. When the tighten fails, the error is
/// returned instead of the handle: logging sensitive content into a file whose
/// permissions could not be restricted is the outcome this function exists to
/// prevent.
#[cfg(unix)]
fn open_log_file(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;

    Ok(file)
}

/// Opens the log file for append. Windows has no mode bits; the file inherits
/// the parent directory's ACL (the user profile).
#[cfg(not(unix))]
fn open_log_file(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
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

    // Deliberately untested: `init` and `app_boot_up` install the global tracing
    // subscriber, which can be set only once per process; `file_layer`'s open path
    // writes into the real user home; the exit helpers terminate the process.

    #[test]
    fn resolve_log_filename_non_rotating_uses_plain_name() {
        let app_logger = AppLogger::new("b64", ToolLogLevel::Warn, false, true, false);

        let path = app_logger.resolve_log_filename();

        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "b64.log");
    }

    #[test]
    fn resolve_log_filename_rotating_wraps_name_and_extension() {
        let app_logger = AppLogger::new("b64", ToolLogLevel::Warn, false, true, true);

        let path = app_logger.resolve_log_filename();

        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("b64-"));
        assert!(name.ends_with(".log"));
    }

    #[test]
    fn resolve_log_filename_parent_is_app_logs_folder() {
        let app_logger = AppLogger::new("b64", ToolLogLevel::Warn, false, true, false);

        let path = app_logger.resolve_log_filename();

        assert!(path.parent().unwrap().ends_with(".b64/logs"));
    }

    #[test]
    fn resolve_log_filename_is_per_app() {
        let b64 = AppLogger::new("b64", ToolLogLevel::Warn, false, true, false);
        let cat = AppLogger::new("cat", ToolLogLevel::Warn, false, true, false);

        let b64_path = b64.resolve_log_filename();
        let cat_path = cat.resolve_log_filename();

        assert_ne!(b64_path, cat_path);
        assert_eq!(b64_path.file_name().unwrap().to_str().unwrap(), "b64.log");
        assert_eq!(cat_path.file_name().unwrap().to_str().unwrap(), "cat.log");
    }

    #[test]
    fn tracing_level_matches_configured_level() {
        assert_eq!(
            AppLogger::new("t", ToolLogLevel::Trace, false, false, false).tracing_level(),
            "trace"
        );
        assert_eq!(
            AppLogger::new("t", ToolLogLevel::Debug, false, false, false).tracing_level(),
            "debug"
        );
        assert_eq!(
            AppLogger::new("t", ToolLogLevel::Info, false, false, false).tracing_level(),
            "info"
        );
        assert_eq!(
            AppLogger::new("t", ToolLogLevel::Warn, false, false, false).tracing_level(),
            "warn"
        );
        assert_eq!(
            AppLogger::new("t", ToolLogLevel::Error, false, false, false).tracing_level(),
            "error"
        );
    }

    #[test]
    fn file_layer_returns_none_when_not_logging_to_file() {
        let app_logger = AppLogger::new("t", ToolLogLevel::Warn, false, false, false);

        assert!(app_logger.file_layer().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn create_log_dir_creates_owner_only_components() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("nested").join("logs");

        create_log_dir(&target).unwrap();

        // mkdir(2) applies `mode & !umask`, so this asserts only that
        // group/other bits are clear; asserting owner bits would fail under an
        // owner-bit-clearing umask.
        for dir in [root.path().join("nested"), target] {
            let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
            assert_eq!(
                mode & 0o077,
                0,
                "created log dir must have no group/other bits: {}",
                dir.display()
            );
        }
    }

    // The file assertions below are exact: open_log_file ends in a
    // handle-based set_permissions (fchmod), which umask does not filter.

    #[cfg(unix)]
    #[test]
    fn open_log_file_creates_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("app.log");

        open_log_file(&path).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "created log file must be owner-only");
    }

    #[cfg(unix)]
    #[test]
    fn open_log_file_tightens_pre_existing_wide_file() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("app.log");
        std::fs::write(&path, b"old content").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        open_log_file(&path).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "pre-existing log file must be tightened on open"
        );
    }

    #[test]
    fn seed_rust_log_sets_when_unset_and_preserves_when_present() {
        // RUST_LOG is process-global and tests run in parallel; this is the only
        // test that touches it, and it restores the original value at the end.
        let original = std::env::var("RUST_LOG").ok();

        // FIXME: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("RUST_LOG") };
        AppLogger::new("t", ToolLogLevel::Info, false, false, false).seed_rust_log();
        assert_eq!(std::env::var("RUST_LOG").unwrap(), "info");

        // FIXME: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("RUST_LOG", "debug") };
        AppLogger::new("t", ToolLogLevel::Info, false, false, false).seed_rust_log();
        assert_eq!(std::env::var("RUST_LOG").unwrap(), "debug");

        match original {
            // FIXME: Audit that the environment access only happens in single-threaded code.
            Some(value) => unsafe { std::env::set_var("RUST_LOG", value) },
            // FIXME: Audit that the environment access only happens in single-threaded code.
            None => unsafe { std::env::remove_var("RUST_LOG") },
        }
    }
}
