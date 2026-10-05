pub mod files_tab;
pub mod keys_tab;
pub mod picker;
pub mod text_tab;

use crate::worker::JobRequest;
use std::path::PathBuf;

/// What a view asks the shell to do. Views render, validate, and open native
/// dialogs; the app submits jobs and touches the filesystem.
pub enum ViewIntent {
    Submit(JobRequest),
    /// Paths picked in the file dialog, handed over for intake (the size
    /// probe and dedup happen in the app).
    AddFiles(Vec<PathBuf>),
}
