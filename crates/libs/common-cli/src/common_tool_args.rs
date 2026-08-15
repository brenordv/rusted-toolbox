use crate::tool_log_level::ToolLogLevel;
use clap::Args;
use crate::app_logger::AppLogger;

#[derive(Args, Debug)]
#[command(about, long_about, version)]
pub struct CommonToolArgs {

    /// Shows the header with tool name, version, and runtime options
    #[arg(long = "app-header")]
    app_header: bool,

    /// Enables verbose output
    #[arg(long = "verbose")]
    verbose: bool,

    /// Sets the default logging level
    #[arg(short='L', long = "log-level", default_value = "error")]
    pub default_logging_level: ToolLogLevel,

    /// If the tool should log to the console
    #[arg(long = "log-to-console")]
    pub log_to_stdout: bool,

    /// If the tool should log to a file
    #[arg(long = "log-to-file")]
    pub log_to_file: bool,

    /// Rotate the log file by day
    #[arg(long = "rotate-log-file-by-day")]
    pub rotate_log_file_by_day: bool
}

impl CommonToolArgs {
    pub fn initialize_logging(&self, force_disable: bool) {
        let app_logger = AppLogger::new(
            self.default_logging_level.clone(),
            self.log_to_stdout,
            self.log_to_file,
            self.rotate_log_file_by_day
        );
        
        app_logger.init(force_disable);
    }
}