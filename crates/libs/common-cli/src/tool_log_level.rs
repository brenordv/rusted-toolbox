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
            ToolLogLevel::Disabled => "".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_maps_to_empty_string() {
        assert_eq!(ToolLogLevel::Disabled.to_tracing_level(), "");
    }

    #[test]
    fn levels_map_to_tracing_names() {
        assert_eq!(ToolLogLevel::Trace.to_tracing_level(), "trace");
        assert_eq!(ToolLogLevel::Debug.to_tracing_level(), "debug");
        assert_eq!(ToolLogLevel::Info.to_tracing_level(), "info");
        assert_eq!(ToolLogLevel::Warn.to_tracing_level(), "warn");
        assert_eq!(ToolLogLevel::Error.to_tracing_level(), "error");
    }
}
