use clap::ValueEnum;
use std::fmt;
use std::path::PathBuf;

pub enum LookupCommand {
    Text(TextLookupConfig),
    Files(FilesLookupConfig),
}

pub struct TextLookupConfig {
    pub path: PathBuf,
    pub text: String,
    pub file_extensions: Vec<String>,
    pub current_only: bool,
    pub line_only: bool,
    pub no_summary: bool,
}

impl TextLookupConfig {
    pub fn new(
        path: PathBuf,
        text: String,
        file_extensions: Vec<String>,
        current_only: bool,
        line_only: bool,
        no_summary: bool,
    ) -> Self {
        Self {
            path,
            text,
            file_extensions,
            current_only,
            line_only,
            no_summary,
        }
    }
}

#[derive(ValueEnum, Debug, PartialEq, Clone)]
pub enum PatternMode {
    Wildcard,
    Regex,
}

// Display feeds clap's `default_value_t`, which parses the rendered text
// back through the ValueEnum, so it must produce the kebab-case value names.
impl fmt::Display for PatternMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PatternMode::Wildcard => write!(f, "wildcard"),
            PatternMode::Regex => write!(f, "regex"),
        }
    }
}

pub struct FilesLookupConfig {
    pub path: PathBuf,
    pub patterns: Vec<String>,
    pub pattern_mode: PatternMode,
    pub case_sensitive: bool,
    pub no_recursive: bool,
    pub no_progress: bool,
    pub no_errors: bool,
    pub no_summary: bool,
}

impl FilesLookupConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        path: PathBuf,
        patterns: Vec<String>,
        pattern_mode: PatternMode,
        case_sensitive: bool,
        no_recursive: bool,
        no_progress: bool,
        no_errors: bool,
        no_summary: bool,
    ) -> Self {
        Self {
            path,
            patterns,
            pattern_mode,
            case_sensitive,
            no_recursive,
            no_progress,
            no_errors,
            no_summary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_mode_display_matches_the_value_enum_names() {
        // clap's default_value_t parses the Display output back through the
        // ValueEnum, so these must stay the kebab-case value names.
        assert_eq!(PatternMode::Wildcard.to_string(), "wildcard");
        assert_eq!(PatternMode::Regex.to_string(), "regex");
    }
}
