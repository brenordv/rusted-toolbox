use std::path::PathBuf;

#[derive(Debug)]
pub struct GitIgnoreConfig {
    pub target_folder: PathBuf,
    pub include_ai: bool,
}
