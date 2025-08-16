use std::path::PathBuf;

#[derive(Debug)]
pub struct ServerArgs {
    pub(crate) root_path: PathBuf,
    pub(crate) port: u16,
}
