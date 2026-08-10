use std::io;

use camino::Utf8Path;
use hurl::runner::{self, RunnerOptionsBuilder, Value, VariableSet};
use hurl::util::logger::{LoggerOptionsBuilder, Verbosity};
use hurl::util::path::ContextDir;
use hurl_core::input::Input;
use thiserror::Error;

use crate::vars::VariableMap;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("failed to determine current directory: {source}")]
    CurrentDir {
        #[source]
        source: io::Error,
    },
    #[error("Hurl runner error: {message}")]
    Runner { message: String },
}

pub fn run_hurl(
    merged: &str,
    display_path: &str,
    variables: &VariableMap,
    verbosity: u8,
    file_root: Option<&Utf8Path>,
) -> Result<runner::HurlResult, EngineError> {
    let variable_set = build_variable_set(variables);

    let mut runner_options = RunnerOptionsBuilder::new();
    runner_options.follow_location(true);

    if let Some(root) = file_root {
        let current_dir =
            std::env::current_dir().map_err(|source| EngineError::CurrentDir { source })?;
        let context_dir = ContextDir::new(&current_dir, root.as_std_path());
        runner_options.context_dir(&context_dir);
    }

    let runner_options = runner_options.build();

    let mut logger_options = LoggerOptionsBuilder::new();
    logger_options.verbosity(match verbosity {
        0 => None,
        1 => Some(Verbosity::Verbose),
        _ => Some(Verbosity::VeryVerbose),
    });
    let logger_options = logger_options.build();

    let input = Input::new(display_path);
    runner::run(
        merged,
        Some(&input),
        &runner_options,
        &variable_set,
        &logger_options,
    )
    .map_err(|message| EngineError::Runner { message })
}

fn build_variable_set(values: &VariableMap) -> VariableSet {
    let mut set = VariableSet::new();
    for (key, value) in values {
        if is_secret_key(key) {
            set.insert_secret(key.clone(), value.clone());
        } else {
            set.insert(key.clone(), Value::String(value.clone()));
        }
    }
    set
}

fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    ["token", "secret", "password", "authorization"]
        .iter()
        .any(|needle| lower.contains(needle))
}
