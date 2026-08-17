use std::path::PathBuf;

pub struct GetLinesConfig {
    pub search: Vec<String>,
    pub file: PathBuf,
    pub output: Option<PathBuf>,
    pub workers: usize,
    pub hide_line_numbers: bool,
}

#[derive(Clone)]
pub struct LineData {
    pub line_number: usize,
    pub content: String,
}
