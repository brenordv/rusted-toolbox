use std::path::PathBuf;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum InputSource {
    Stdin,
    File(PathBuf),
    Directory(PathBuf),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum OutputTarget {
    Stdout,
    File(PathBuf),
}

#[derive(Debug, Clone)]
pub struct RemoveZwArgs {
    pub inputs: Vec<InputSource>,
    pub output: Option<OutputTarget>,
    pub in_place: bool,
    pub recursive: bool,
    pub extensions: Vec<String>,
    pub verbose: bool,
    pub no_header: bool,
}
