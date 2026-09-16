//! Shared command-line machinery for the toolbox: the common CLI flags every tool
//! flattens in, the logging bootstrap, and the process exit helpers.

pub mod app_logger;
pub mod broken_pipe;
pub mod common_tool_args;
pub mod header_format;
pub mod test_writers;
pub mod tool_exit_helpers;
pub mod tool_log_level;
pub mod tool_path_helpers;
