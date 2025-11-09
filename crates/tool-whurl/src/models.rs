use std::fmt;
use thiserror::Error;

use camino::Utf8PathBuf;

#[derive(Debug)]
pub struct Cli {
    pub command: Command,
}

#[derive(Debug)]
pub enum Command {
    List(ListArgs),
    Run(RunArgs),
    DryRun(DryRunArgs),
}

#[derive(Debug)]
pub struct ListArgs {
    pub api: Option<String>,
}

#[derive(Debug)]
pub struct RunArgs {
    pub exec: ExecutionArgs,
    pub json_output: Option<Utf8PathBuf>,
    pub test_mode: bool,
    pub print_only_result: bool,
    pub silent: bool,
}

#[derive(Debug)]
pub struct DryRunArgs {
    pub exec: ExecutionArgs,
    pub show_boundaries: bool,
}

#[derive(Debug)]
pub struct ExecutionArgs {
    pub api: String,
    pub file: String,
    pub env: Option<String>,
    pub vars_file: Option<Utf8PathBuf>,
    pub inline_vars: Vec<KeyValue>,
    pub file_root: Option<Utf8PathBuf>,
    pub verbosity: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
}

impl fmt::Display for KeyValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.key, self.value)
    }
}

pub type ToolResult<T> = Result<T, ToolError>;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ExitCode {
    Success = 0,
    ExecutionFailure = 1,
    IncludeFailure = 2,
    Unknown = 3,
}

#[derive(Debug, Error)]
pub enum ToolError {
    #[error(transparent)]
    Include(#[from] crate::includer::IncluderError),
    #[error(transparent)]
    Resolve(#[from] crate::files::ResolveError),
    #[error(transparent)]
    Discover(#[from] crate::files::DiscoverError),
    #[error(transparent)]
    Vars(#[from] crate::vars::VariableError),
    #[error(transparent)]
    Engine(#[from] crate::engine::EngineError),
    #[error(transparent)]
    Output(#[from] crate::output::OutputError),
    #[error("{0}")]
    Other(#[from] anyhow::Error),
    #[error("Hurl execution reported one or more failures")]
    ExecutionFailure,
}

impl ToolError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            ToolError::Include(_) | ToolError::Resolve(_) => ExitCode::IncludeFailure,
            ToolError::ExecutionFailure => ExitCode::ExecutionFailure,
            _ => ExitCode::Unknown,
        }
    }
}

impl From<ExitCode> for i32 {
    fn from(code: ExitCode) -> Self {
        match code {
            ExitCode::Success => 0,
            ExitCode::ExecutionFailure => 1,
            ExitCode::IncludeFailure => 2,
            ExitCode::Unknown => 3,
        }
    }
}
