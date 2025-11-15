use std::{env, fs, io, path::PathBuf};

use crate::files::{FileResolver, ResolveError, ResolvedRunContext};
use crate::models::ToolResult;
use crate::vars::{parse_dynamic_variables_file, parse_variables_file, VariableMap};
use camino::{Utf8Path, Utf8PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiscoverError {
    #[error("WHURL_REQUEST_HOME must be a valid UTF-8 path")]
    InvalidUtf8Env,
    #[error("encountered non UTF-8 path")]
    NonUtf8Path(PathBuf),
    #[error("failed to determine current executable path: {0}")]
    CurrentExe(io::Error),
    #[error("unable to derive parent directory from current executable")]
    ExecutableWithoutParent,
    #[error("requests directory not found at {0}")]
    RequestsDirMissing(Utf8PathBuf),
    #[error("API '{api}' not found under {root}")]
    ApiNotFound { api: String, root: Utf8PathBuf },
    #[error("failed to read directory {path}: {source}")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
}

pub fn locate_requests_root() -> Result<Utf8PathBuf, DiscoverError> {
    if let Ok(env_path) = env::var("WHURL_REQUEST_HOME") {
        let root = Utf8PathBuf::from(env_path);
        if !root.is_dir() {
            return Err(DiscoverError::RequestsDirMissing(root));
        }

        return Ok(root);
    }

    let exe_path = env::current_exe().map_err(DiscoverError::CurrentExe)?;
    let Some(exe_dir) = exe_path.parent() else {
        return Err(DiscoverError::ExecutableWithoutParent);
    };

    let exe_dir_utf8 = Utf8PathBuf::from_path_buf(exe_dir.to_path_buf())
        .map_err(|_| DiscoverError::InvalidUtf8Env)?;

    let root = exe_dir_utf8.join("requests");
    if root.is_dir() {
        return Ok(root);
    }

    Err(DiscoverError::RequestsDirMissing(root))
}

pub fn list_apis(requests_root: &Utf8Path) -> Result<Vec<String>, DiscoverError> {
    let mut apis = Vec::new();

    let entries = fs::read_dir(requests_root).map_err(|source| DiscoverError::Io {
        path: requests_root.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| DiscoverError::Io {
            path: requests_root.to_path_buf(),
            source,
        })?;
        let raw_path = entry.path();
        let utf8_path =
            Utf8PathBuf::from_path_buf(raw_path.clone()).map_err(DiscoverError::NonUtf8Path)?;

        let metadata = entry.metadata().map_err(|source| DiscoverError::Io {
            path: utf8_path.clone(),
            source,
        })?;

        if metadata.is_dir() {
            if let Some(name) = utf8_path.file_name() {
                apis.push(name.to_string());
            }
        }
    }

    apis.sort_unstable();
    Ok(apis)
}

pub fn list_requests(requests_root: &Utf8Path, api: &str) -> Result<Vec<String>, DiscoverError> {
    let api_root = requests_root.join(api);
    if !api_root.is_dir() {
        return Err(DiscoverError::ApiNotFound {
            api: api.to_string(),
            root: requests_root.to_path_buf(),
        });
    }

    let mut requests = Vec::new();

    let entries = fs::read_dir(&api_root).map_err(|source| DiscoverError::Io {
        path: api_root.clone(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| DiscoverError::Io {
            path: api_root.clone(),
            source,
        })?;
        let path = Utf8PathBuf::from_path_buf(entry.path()).map_err(DiscoverError::NonUtf8Path)?;

        if path.is_file() && path.extension() == Some("hurl") {
            if let Some(name) = path.file_stem() {
                requests.push(name.to_string());
            }
        }
    }

    requests.sort_unstable();
    Ok(requests)
}

pub fn resolve_vars_file_path(api_root: &Utf8Path, vars_file: &Utf8PathBuf) -> Utf8PathBuf {
    if vars_file.is_absolute() {
        vars_file.clone()
    } else if vars_file.as_path().is_file() {
        vars_file.clone()
    } else {
        let candidate = api_root.join(vars_file);
        if candidate.as_path().is_file() {
            candidate
        } else {
            vars_file.clone()
        }
    }
}

pub fn resolve_file_root(
    context: &ResolvedRunContext,
    file_root: Option<&Utf8PathBuf>,
) -> Option<Utf8PathBuf> {
    let file_root = file_root?;

    if file_root.is_absolute() {
        Some(file_root.clone())
    } else {
        Some(context.resolution.api_root.join(file_root))
    }
}

pub fn load_env_file(
    resolver: &FileResolver,
    api: &str,
    env_name: &str,
    required: bool,
) -> ToolResult<Option<(Utf8PathBuf, VariableMap)>> {
    match resolver.resolve_env_file(api, env_name) {
        Ok(path) => {
            let vars = parse_variables_file(path.as_path())?;
            Ok(Some((path, vars)))
        }
        Err(ResolveError::FileNotFound { .. }) if !required => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn load_dynamic_vars_file(
    resolver: &FileResolver,
    api: &str,
    vars_name: &str,
    required: bool,
    allow_shell: bool,
    log_assignments: bool,
) -> ToolResult<Option<(Utf8PathBuf, VariableMap)>> {
    match resolver.resolve_dynamic_vars_file(api, vars_name) {
        Ok(path) => {
            let vars = parse_dynamic_variables_file(path.as_path(), allow_shell, log_assignments)?;
            Ok(Some((path, vars)))
        }
        Err(ResolveError::FileNotFound { .. }) if !required => Ok(None),
        Err(error) => Err(error.into()),
    }
}
