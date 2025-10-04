pub struct LookupConfig {
    pub path: String,
    pub text: String,
    pub file_extensions: Vec<String>,
    pub no_header: bool,
    pub current_only: bool,
    pub line_only: bool,
}

impl LookupConfig {
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
