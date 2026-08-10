use std::net::IpAddr;
use std::path::PathBuf;

#[derive(Debug)]
pub struct ServerArgs {
    pub(crate) root_path: PathBuf,
    pub(crate) port: u16,
    pub(crate) host: IpAddr,
    pub(crate) serve_hidden: bool,
}

/// A directory entry: `(name, relative_url_path)`.
pub type DirEntry = (String, String);

/// A file entry: `(name, relative_url_path, size_in_bytes)`.
pub type FileEntry = (String, String, u64);
