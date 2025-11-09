// High-level command selected from CLI
pub enum LookupCommand {
    Text(TextLookupConfig),
    Files(FilesLookupConfig),
}

// Config for `lookup text` subcommand
pub struct TextLookupConfig {
    pub path: String,
    pub text: String,
    pub file_extensions: Vec<String>,
    pub no_header: bool,
    pub current_only: bool,
    pub line_only: bool,
}

impl TextLookupConfig {
    pub fn new(
        path: String,
        text: String,
        file_extensions: Vec<String>,
        no_header: bool,
        current_only: bool,
        line_only: bool,
    ) -> Self {
        Self {
            path,
            text,
            file_extensions,
            no_header,
            current_only,
            line_only,
        }
    }
}

// Pattern type for `files` subcommand
#[derive(Clone, Copy)]
pub enum PatternMode {
    Wildcard,
    Regex,
}

// Config for `lookup files` subcommand
pub struct FilesLookupConfig {
    pub path: String,
    pub patterns: Vec<String>,
    pub pattern_mode: PatternMode,
    pub case_sensitive: bool,
    pub recursive: bool,
    pub no_header: bool,
    pub no_progress: bool,
    pub no_errors: bool,
    pub no_summary: bool,
}

impl FilesLookupConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        path: String,
        patterns: Vec<String>,
        pattern_mode: PatternMode,
        case_sensitive: bool,
        recursive: bool,
        no_header: bool,
        no_progress: bool,
        no_errors: bool,
        no_summary: bool,
    ) -> Self {
        Self {
            path,
            patterns,
            pattern_mode,
            case_sensitive,
            recursive,
            no_header,
            no_progress,
            no_errors,
            no_summary,
        }
    }
}
