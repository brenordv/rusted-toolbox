use crate::logging::app_logger::{AppLogger, LogLevel};

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

pub fn get_default_log_builder(app_name: &str, log_level: LogLevel) -> AppLogger {
    let mut builder = AppLogger::new(true);

    builder
        .log_level(log_level)
        .app_name(app_name)
        .log_to_console(true);

    builder
}
