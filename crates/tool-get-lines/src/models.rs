pub struct GetLinesArgs {
    pub search: Vec<String>,
    pub file: String,
    pub output: Option<String>,
    pub workers: usize,
    pub hide_line_numbers: bool,
    pub hide_runtime_info: bool,
}

#[derive(Clone)]
pub struct LineData {
    pub line_number: usize,
    pub content: String,
}
