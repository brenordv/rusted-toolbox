mod json;
mod summarize;

pub use json::write_json_report;
pub use summarize::print_test_summary;

use std::io;

use camino::Utf8PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OutputError {
    #[error("failed to create directory {path}: {source}")]
    CreateDir {
        path: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to write output to {target}: {source}")]
    StreamWrite {
        target: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to read source file {path}: {source}")]
    SourceRead {
        path: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
}
