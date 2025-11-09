mod collect;
mod envfile;

pub use collect::{gather_process_env_variables, merge_variable_sources};
pub use envfile::parse_variables_file;

use std::collections::BTreeMap;
use std::io;

use camino::Utf8PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VariableError {
    #[error("failed to read variables file {path}: {source}")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid variable declaration in {path} at line {line}: {message}")]
    Parse {
        path: Utf8PathBuf,
        line: usize,
        message: String,
    },
}

pub type VariableMap = BTreeMap<String, String>;
