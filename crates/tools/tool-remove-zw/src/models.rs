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
    pub dry_run: bool,
    pub check: bool,
    pub keep_bom: bool,
}

impl RemoveZwArgs {
    /// True in the modes that only report (dry run and check) and never write.
    pub fn report_only(&self) -> bool {
        self.dry_run || self.check
    }
}
