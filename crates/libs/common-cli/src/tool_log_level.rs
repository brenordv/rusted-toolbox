use clap::ValueEnum;

#[derive(ValueEnum, Debug, PartialEq, Clone)]
pub enum ToolLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Disabled,
}

impl ToolLogLevel {
    /// Converts the current `LogLevel` instance to its corresponding tracing log level.
    ///
    /// # Returns
    /// A `String` that contains the log level compatible with tracing.
    pub fn to_tracing_level(&self) -> String {
        match self {
            ToolLogLevel::Trace => "trace".to_string(),
            ToolLogLevel::Debug => "debug".to_string(),
            ToolLogLevel::Info => "info".to_string(),
            ToolLogLevel::Warn => "warn".to_string(),
            ToolLogLevel::Error => "error".to_string(),
            ToolLogLevel::Disabled => "".to_string()
        }
    }
}