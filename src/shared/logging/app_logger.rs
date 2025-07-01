#![allow(dead_code)] // This module is used by other modules, so the code is not really dead.

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

/// Represents the severity levels of a log message in an application.
///
/// This enum categorizes log messages into six distinct levels based on their importance
/// or severity, ranging from highly detailed messages meant for debugging to critical
/// messages indicating fatal errors.
///
/// # Variants
///
/// - `Trace`
///   - Designates highly detailed and verbose log messages, typically used for tracing the
///     execution flow or providing insights during deep debugging.
/// - `Debug`
///   - Represents messages intended for debugging purposes, containing diagnostic information
///     useful for developers during development.
/// - `Info`
///   - Denotes informational messages that highlight the normal operation or key milestones
///     of an application.
/// - `Warn`
///   - Indicates situations that may lead to potential problems or require attention, but
///     are not yet errors.
/// - `Error`
///   - Represents an error that has occurred, preventing a specific operation from succeeding.
///     However, it might not necessarily halt the entire application.
/// - `Fatal`
///   - Signifies critical errors that lead to the immediate termination or failure of the entire
///     application or system.
///
/// This enum can be used in logging frameworks to filter, format, or prioritize log messages
/// based on their severity.
#[derive(Debug, PartialEq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    /// Converts the current `LogLevel` instance to its corresponding default log level for a
    /// specific application and returns the formatted string.
    ///
    /// # Arguments
    /// * `app_name` - A reference to a `String` that represents the name of the application.
    ///
    /// # Returns
    /// A `String` that contains the application name and the log level in the
    /// format `<app_name>=<log_level>`.
    pub fn to_default_log_level_for_app(&self, app_name: &String) -> String {
        let log_level = match self {
            LogLevel::Trace => "trace".to_string(),
            LogLevel::Debug => "debug".to_string(),
            LogLevel::Info => "info".to_string(),
            LogLevel::Warn => "warn".to_string(),
            LogLevel::Error => "error".to_string(),
            LogLevel::Fatal => "fatal".to_string(),
        };

        format!("{}={}", app_name, log_level)
    }
}

/// The `AppLogger` struct is a customizable logging utility designed to manage
/// logging configurations for an application. It allows for toggling log outputs,
/// log level control, file-based logging, and log file rotation.
///
/// # Fields
///
/// - `enabled` (`bool`):
///   Specifies whether logging is enabled for the application.
///   - `true`: Logging is active and events will be logged.
///   - `false`: Logging is disabled.
///
/// - `to_console` (`bool`):
///   Determines whether logs will be output to the console (stdout).
///   - `true`: Logs will be printed to the console.
///   - `false`: Logs will not be printed to the console.
///
/// - `to_file` (`bool`):
///   Indicates whether logs should be written to a file.
///   - `true`: Logs will be saved in a file.
///   - `false`: Logs will not be saved in a file.
///
/// - `rotate_file_by_day` (`bool`):
///   Enables or disables log file rotation based on the day.
///   - `true`: A separate log file will be created for each day, ensuring logs are split by date.
///   - `false`: Log file rotation by day is disabled.
///
/// - `log_level` (`LogLevel`):
///   Represents the minimum level of logs to record. Higher priority logs (e.g., errors)
///   will also be logged, but lower priority logs will be filtered out.
///   Common log levels might include: Error, Warning, Info, Debug, etc.
///
/// - `log_folder` (`String`):
///   Specifies the directory where log files will be stored. This is only relevant if
///   `to_file` is set to `true`.
///
/// - `app_name` (`String`):
///   Represents the name of the application, which may be used in log messages or
///   as part of the log file naming convention.
pub struct AppLogger {
    enabled: bool,
    to_console: bool, //stdout
    to_file: bool,
    rotate_file_by_day: bool,
    log_level: LogLevel,
    log_folder: String,
    app_name: String,
}

impl AppLogger {
    /// Creates a new instance of the structure with the default logging configuration.
    ///
    /// # Parameters
    /// - `enabled` (bool): Determines whether logging is enabled or disabled.
    ///
    /// # Returns
    /// A new instance with the following default values:
    /// - `enabled`: Set based on the input parameter.
    /// - `to_console`: `true` - Logs will be outputted to the console by default.
    /// - `to_file`: `false` - File logging is disabled by default.
    /// - `rotate_file_by_day`: `false` - Log file rotation by day is disabled by default.
    /// - `log_level`: `LogLevel::Info` - Default logging level is set to `Info`.
    /// - `log_folder`: `"logs"` - Default folder name for storing log files.
    /// - `app_name`: `"App"` - Default application name for the logging system.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            to_console: true,
            to_file: false,
            rotate_file_by_day: false,
            log_level: LogLevel::Info,
            log_folder: "logs".to_string(),
            app_name: "App".to_string(),
        }
    }

    /// Configures whether logging should output to the console.
    ///
    /// # Parameters
    /// - `to_console`: A boolean value indicating if logs should be written to the console.
    ///   - `true` enables logging to the console.
    ///   - `false` disables logging to the console.
    ///
    /// # Returns
    /// - Returns a mutable reference to the current instance of the struct, allowing method
    /// chaining.
    pub fn log_to_console(&mut self, to_console: bool) -> &mut Self {
        self.to_console = to_console;
        self
    }

    ///
    /// Configures logging behavior to write logs to a file and determines
    /// whether log files should be rotated daily.
    ///
    /// # Parameters
    /// - `to_file`: A boolean value indicating whether logs should be written to a file.
    ///   - `true` enables logging to a file.
    ///   - `false` disables it.
    /// - `rotate_log_by_day`: A boolean value specifying whether the log file should rotate daily.
    ///   - `true` enables daily log rotation.
    ///   - `false` disables it.
    ///
    /// # Returns
    /// Returns a mutable reference to `Self`, allowing method chaining for further configuration.
    pub fn log_to_file(&mut self, to_file: bool, rotate_log_by_day: bool) -> &mut Self {
        self.to_file = to_file;
        self.rotate_file_by_day = rotate_log_by_day;
        self
    }

    /// Sets the log folder path for the instance.
    ///
    /// This method allows you to specify the folder path where logs will be stored.
    /// The provided `log_folder` string is converted into an owned `String` and
    /// assigned to the instance's `log_folder` property.
    ///
    /// # Arguments
    ///
    /// * `log_folder` - A string slice that specifies the path of the log folder.
    ///
    /// # Returns
    ///
    /// Returns a mutable reference to `self`, enabling method chaining.
    pub fn log_folder(&mut self, log_folder: &str) -> &mut Self {
        self.log_folder = log_folder.to_string();
        self
    }

    /// Sets the name of the application.
    ///
    /// This method updates the `app_name` field of the struct with the provided
    /// string. The input string is converted into an owned `String` and stored
    /// internally. The method returns a mutable reference to the current instance,
    /// allowing for method chaining.
    ///
    /// # Arguments
    ///
    /// * `app_name` - A reference to a string slice representing the name of the
    ///   application to be set.
    ///
    /// # Returns
    ///
    /// A mutable reference to the current instance (`Self`), enabling method
    /// chaining.
    pub fn app_name(&mut self, app_name: &str) -> &mut Self {
        self.app_name = app_name.to_string();
        self
    }

    /// Sets the log level for the current instance and returns a mutable reference to the instance.
    ///
    /// # Parameters
    /// - `log_level`: The log level to set, represented by the `LogLevel` enum. This determines
    /// the severity or verbosity of logs that will be handled.
    ///
    /// # Returns
    /// A mutable reference to the current instance, allowing for method chaining.
    pub fn log_level(&mut self, log_level: LogLevel) -> &mut Self {
        self.log_level = log_level;
        self
    }

    /// Initializes and configures the application's logging infrastructure.
    ///
    /// This method sets up the logging system using the `tracing_subscriber` library.
    /// It dynamically configures logging behavior based on the provided settings, enabling
    /// developers to output logs to the console and/or a file. It also applies filters for log
    /// levels and supports file rotation by day.
    ///
    /// # Behavior
    /// - If `self.enabled` is `false`, logging is skipped, and no further setup is performed.
    /// - Reads the log level configuration from the environment variable or uses a default value.
    /// - Configures logging layers for console output and/or file output based on user preferences.
    /// - Rotates log files by day if `self.rotate_file_by_day` is `true`.
    /// - Ensures the `log_folder` directory exists; creates it if missing.
    /// - Disables ANSI color codes for logs written to files.
    ///
    /// # Requirements
    /// - The `self.app_name` field should contain a valid name for the application, which is used
    /// in log file names.
    /// - The `self.log_folder` specifies the directory where log files will be saved.
    /// - Environmental log level configuration should be provided (optional).
    ///
    /// # Fields/Configuration
    /// - `self.enabled`: A `bool` flag to enable or disable logging setup.
    /// - `self.log_level`: Defines the log level for filtering messages (e.g., Error, Info, etc.).
    /// - `self.app_name`: The name of the application, used for labeling log files.
    /// - `self.log_folder`: The folder where log files will be created and saved.
    /// - `self.to_console`: A `bool` flag indicating whether logs should be output to the console.
    /// - `self.to_file`: A `bool` flag indicating whether logs should be written to a file.
    /// - `self.rotate_file_by_day`: A `bool` flag to enable daily log rotation.
    ///
    /// # Errors
    /// - If the `tracing_subscriber` environment filter cannot be initialized, it falls back to a
    ///   default log level defined in `self.log_level`.
    /// - If the log folder cannot be created or file handling fails, those logs are
    /// silently ignored.
    ///
    /// # Notes
    /// - It is recommended to provide appropriate permissions for the `log_folder` path to ensure
    ///   the logger can create and write log files.
    /// - Log settings dynamically adapt to environmental configuration where supported.
    pub fn init(&self) {
        if !self.enabled {
            return;
        }

        let default_log_level = self.log_level.to_default_log_level_for_app(&self.app_name);

        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| default_log_level.into());

        let mut layers = Vec::new();

        // Add console layer if enabled
        if self.to_console {
            layers.push(tracing_subscriber::fmt::layer().boxed());
        }

        // Add file layer if enabled
        if self.to_file {
            let file_name = if self.rotate_file_by_day {
                format!(
                    "{}-{}.log",
                    self.app_name,
                    chrono::Utc::now().format("%Y-%m-%d")
                )
            } else {
                format!("{}.log", self.app_name)
            };

            // Create a log directory if it doesn't exist
            if let Ok(log_dir) = std::env::current_dir().map(|d| d.join(&self.log_folder)) {
                let _ = std::fs::create_dir_all(&log_dir);

                if let Ok(file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(log_dir.join(file_name))
                {
                    let file_layer = tracing_subscriber::fmt::layer()
                        .with_writer(file)
                        .with_ansi(false) // Disable ANSI colors for file output
                        .boxed();
                    layers.push(file_layer);
                }
            }
        }

        // Build and initialize the subscriber
        let subscriber = tracing_subscriber::registry().with(env_filter).with(layers);

        subscriber.init();
    }

    /*
     * Helper methods for testing
     *
     * These methods are used to test the functionality of the `AppLogger` struct.
     * They are not meant to (and will not be available) be used in production code.
     */

    #[cfg(test)]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    #[cfg(test)]
    pub fn logs_to_console(&self) -> bool {
        self.to_console
    }

    #[cfg(test)]
    pub fn logs_to_file(&self) -> bool {
        self.to_file
    }

    #[cfg(test)]
    pub fn rotates_file_by_day(&self) -> bool {
        self.rotate_file_by_day
    }

    #[cfg(test)]
    pub fn get_log_folder(&self) -> &str {
        &self.log_folder
    }

    #[cfg(test)]
    pub fn get_app_name(&self) -> &str {
        &self.app_name
    }

    #[cfg(test)]
    pub fn get_log_level(&self) -> &LogLevel {
        &self.log_level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_default_configuration() {
        let logger = AppLogger::new(true);

        assert!(logger.is_enabled());
        assert!(logger.logs_to_console());
        assert!(!logger.logs_to_file());
        assert!(!logger.rotates_file_by_day());
        assert_eq!(logger.get_log_folder(), "logs");
        assert_eq!(logger.get_app_name(), "App");
        assert_eq!(logger.get_log_level(), &LogLevel::Info);
    }

    #[test]
    fn test_new_with_disabled_logging() {
        let logger = AppLogger::new(false);

        assert!(!logger.is_enabled());
        assert!(logger.logs_to_console());
        assert!(!logger.logs_to_file());
        assert!(!logger.rotates_file_by_day());
        assert_eq!(logger.get_log_folder(), "logs");
        assert_eq!(logger.get_app_name(), "App");
        assert_eq!(logger.get_log_level(), &LogLevel::Info);
    }

    #[test]
    fn test_log_to_console_method_chaining() {
        let mut logger = AppLogger::new(true);

        let result = logger.log_to_console(false);

        // Verify method chaining works
        assert!(std::ptr::eq(result, &mut logger));
        assert!(!logger.logs_to_console());
    }

    #[test]
    fn test_log_to_console_enables_console_logging() {
        let mut logger = AppLogger::new(true);
        logger.log_to_console(false);

        assert!(!logger.logs_to_console());

        logger.log_to_console(true);
        assert!(logger.logs_to_console());
    }

    #[test]
    fn test_log_to_file_method_chaining() {
        let mut logger = AppLogger::new(true);

        let result = logger.log_to_file(true, false);

        // Verify method chaining works
        assert!(std::ptr::eq(result, &mut logger));
        assert!(logger.logs_to_file());
        assert!(!logger.rotates_file_by_day());
    }

    #[test]
    fn test_log_to_file_with_rotation() {
        let mut logger = AppLogger::new(true);

        logger.log_to_file(true, true);

        assert!(logger.logs_to_file());
        assert!(logger.rotates_file_by_day());
    }

    #[test]
    fn test_log_to_file_without_rotation() {
        let mut logger = AppLogger::new(true);

        logger.log_to_file(true, false);

        assert!(logger.logs_to_file());
        assert!(!logger.rotates_file_by_day());
    }

    #[test]
    fn test_log_folder_method_chaining() {
        let mut logger = AppLogger::new(true);

        let result = logger.log_folder("custom_logs");

        // Verify method chaining works
        assert!(std::ptr::eq(result, &mut logger));
        assert_eq!(logger.get_log_folder(), "custom_logs");
    }

    #[test]
    fn test_log_folder_updates_path() {
        let mut logger = AppLogger::new(true);

        logger.log_folder("/var/log/myapp");
        assert_eq!(logger.get_log_folder(), "/var/log/myapp");

        logger.log_folder("./logs");
        assert_eq!(logger.get_log_folder(), "./logs");
    }

    #[test]
    fn test_app_name_method_chaining() {
        let mut logger = AppLogger::new(true);

        let result = logger.app_name("MyApplication");

        // Verify method chaining works
        assert!(std::ptr::eq(result, &mut logger));
        assert_eq!(logger.get_app_name(), "MyApplication");
    }

    #[test]
    fn test_app_name_updates_name() {
        let mut logger = AppLogger::new(true);

        logger.app_name("TestApp");
        assert_eq!(logger.get_app_name(), "TestApp");

        logger.app_name("AnotherApp");
        assert_eq!(logger.get_app_name(), "AnotherApp");
    }

    #[test]
    fn test_log_level_method_chaining() {
        let mut logger = AppLogger::new(true);

        let result = logger.log_level(LogLevel::Debug);

        // Verify method chaining works
        assert!(std::ptr::eq(result, &mut logger));
    }

    #[test]
    fn test_builder_pattern_chaining() {
        let mut logger = AppLogger::new(true);

        logger
            .app_name("ChainedApp")
            .log_folder("chained_logs")
            .log_to_console(false)
            .log_to_file(true, true)
            .log_level(LogLevel::Error);

        assert_eq!(logger.get_app_name(), "ChainedApp");
        assert_eq!(logger.get_log_folder(), "chained_logs");
        assert!(!logger.logs_to_console());
        assert!(logger.logs_to_file());
        assert!(logger.rotates_file_by_day());
    }

    #[test]
    fn test_build_returns_early_when_disabled() {
        let logger = AppLogger::new(false);

        // This should not panic or cause issues
        logger.init();

        // If we get here, the test passed (no panic occurred)
        assert!(true);
    }

    #[test]
    fn test_log_level_to_default_log_level_for_app_trace() {
        let log_level = LogLevel::Trace;
        let app_name = "TestApp".to_string();

        let result = log_level.to_default_log_level_for_app(&app_name);

        assert_eq!(result, "TestApp=trace");
    }

    #[test]
    fn test_log_level_to_default_log_level_for_app_debug() {
        let log_level = LogLevel::Debug;
        let app_name = "TestApp".to_string();

        let result = log_level.to_default_log_level_for_app(&app_name);

        assert_eq!(result, "TestApp=debug");
    }

    #[test]
    fn test_log_level_to_default_log_level_for_app_info() {
        let log_level = LogLevel::Info;
        let app_name = "TestApp".to_string();

        let result = log_level.to_default_log_level_for_app(&app_name);

        assert_eq!(result, "TestApp=info");
    }

    #[test]
    fn test_log_level_to_default_log_level_for_app_warn() {
        let log_level = LogLevel::Warn;
        let app_name = "TestApp".to_string();

        let result = log_level.to_default_log_level_for_app(&app_name);

        assert_eq!(result, "TestApp=warn");
    }

    #[test]
    fn test_log_level_to_default_log_level_for_app_error() {
        let log_level = LogLevel::Error;
        let app_name = "TestApp".to_string();

        let result = log_level.to_default_log_level_for_app(&app_name);

        assert_eq!(result, "TestApp=error");
    }

    #[test]
    fn test_log_level_to_default_log_level_for_app_fatal() {
        let log_level = LogLevel::Fatal;
        let app_name = "TestApp".to_string();

        let result = log_level.to_default_log_level_for_app(&app_name);

        assert_eq!(result, "TestApp=fatal");
    }

    #[test]
    fn test_log_level_with_different_app_names() {
        let log_level = LogLevel::Info;

        assert_eq!(
            log_level.to_default_log_level_for_app(&"App1".to_string()),
            "App1=info"
        );

        assert_eq!(
            log_level.to_default_log_level_for_app(&"MyService".to_string()),
            "MyService=info"
        );

        assert_eq!(
            log_level.to_default_log_level_for_app(&"web-server".to_string()),
            "web-server=info"
        );
    }

    #[test]
    fn test_log_level_with_empty_app_name() {
        let log_level = LogLevel::Debug;
        let app_name = "".to_string();

        let result = log_level.to_default_log_level_for_app(&app_name);

        assert_eq!(result, "=debug");
    }

    #[test]
    fn test_log_level_with_special_characters_in_app_name() {
        let log_level = LogLevel::Warn;
        let app_name = "my-app_v2.0".to_string();

        let result = log_level.to_default_log_level_for_app(&app_name);

        assert_eq!(result, "my-app_v2.0=warn");
    }

    // Integration test for the complete configuration
    #[test]
    fn test_complete_logger_configuration() {
        let mut logger = AppLogger::new(true);

        logger
            .app_name("IntegrationTest")
            .log_folder("test_logs")
            .log_to_console(true)
            .log_to_file(false, false)
            .log_level(LogLevel::Debug);

        assert!(logger.is_enabled());
        assert_eq!(logger.get_app_name(), "IntegrationTest");
        assert_eq!(logger.get_log_folder(), "test_logs");
        assert!(logger.logs_to_console());
        assert!(!logger.logs_to_file());
        assert!(!logger.rotates_file_by_day());
    }

    // Test edge cases
    #[test]
    fn test_multiple_calls_to_same_method() {
        let mut logger = AppLogger::new(true);

        logger.app_name("First");
        logger.app_name("Second");
        logger.app_name("Final");

        assert_eq!(logger.get_app_name(), "Final");
    }

    #[test]
    fn test_toggle_boolean_values() {
        let mut logger = AppLogger::new(true);

        // Test toggling console logging
        logger.log_to_console(false);
        assert!(!logger.logs_to_console());
        logger.log_to_console(true);
        assert!(logger.logs_to_console());

        // Test toggling file logging
        logger.log_to_file(true, false);
        assert!(logger.logs_to_file());
        assert!(!logger.rotates_file_by_day());

        logger.log_to_file(true, true);
        assert!(logger.logs_to_file());
        assert!(logger.rotates_file_by_day());

        logger.log_to_file(false, false);
        assert!(!logger.logs_to_file());
        assert!(!logger.rotates_file_by_day());
    }
}
