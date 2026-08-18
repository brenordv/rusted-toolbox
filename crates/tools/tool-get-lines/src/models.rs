use std::path::PathBuf;

/// Runtime configuration for the get-lines tool.
#[derive(Debug, Clone)]
pub struct GetLinesConfig {
    /// Normalized, deduplicated, lowercase search terms.
    pub search: Vec<String>,
    /// Path to the input file.
    pub file: PathBuf,
    /// Output directory; `None` writes matches to the console.
    pub output: Option<PathBuf>,
    /// When `true`, matched lines are written without a leading line number.
    pub hide_line_numbers: bool,
}
