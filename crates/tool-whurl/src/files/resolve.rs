use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use thiserror::Error;

pub const ENVS_DIR_NAME: &str = "_vars";

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("requests directory does not contain API '{api}'")]
    ApiNotFound { api: String, path: Utf8PathBuf },
    #[error("path escapes the requests root: {path}")]
    OutsideRequestsRoot { path: Utf8PathBuf },
    #[error("file not found: {path}")]
    FileNotFound { path: Utf8PathBuf },
    #[error("invalid path component '{component}'")]
    InvalidComponent { component: String },
}

#[derive(Debug, Clone)]
pub struct FileResolution {
    pub api: String,
    pub api_root: Utf8PathBuf,
    pub file_path: Utf8PathBuf,
}

#[derive(Debug, Clone)]
pub struct ResolvedRunContext {
    pub resolution: FileResolution,
    pub display_path: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedInclude {
    pub path: Utf8PathBuf,
    pub logical: String,
}

#[derive(Debug, Clone)]
pub struct FileResolver {
    requests_root: Utf8PathBuf,
}

impl FileResolver {
    pub fn new(requests_root: Utf8PathBuf) -> Self {
        Self { requests_root }
    }

    pub fn requests_root(&self) -> &Utf8Path {
        &self.requests_root
    }

    pub fn resolve_run_context(
        &self,
        api: &str,
        file: &str,
    ) -> Result<ResolvedRunContext, ResolveError> {
        let api_root = self.resolve_api_root(api)?;
        let file_path = self.resolve_file_path(&api_root, file)?;

        let display_path = file_path
            .strip_prefix(&self.requests_root)
            .unwrap_or(&file_path)
            .to_string();

        Ok(ResolvedRunContext {
            resolution: FileResolution {
                api: api.to_string(),
                api_root,
                file_path,
            },
            display_path,
        })
    }

    pub fn resolve_include(
        &self,
        current_file: &Utf8Path,
        include_spec: &str,
    ) -> Result<ResolvedInclude, ResolveError> {
        let relative = sanitize_relative(include_spec)?;

        let mut candidate_names = Vec::with_capacity(3);

        if let Some(parent_dir) = current_file.parent() {
            candidate_names.push(parent_dir.join(relative.as_path()));
        }

        if let Some(api_root) = self.api_root_for(current_file)? {
            let candidate = api_root.join(relative.as_path());
            if !candidate_names.contains(&candidate) {
                candidate_names.push(candidate);
            }
        }

        let root_candidate = self.requests_root.join(relative.as_path());
        if !candidate_names.contains(&root_candidate) {
            candidate_names.push(root_candidate);
        }

        for mut candidate in candidate_names {
            ensure_extension(&mut candidate);
            if is_within_root(&self.requests_root, &candidate) {
                if candidate.is_file() {
                    let logical = candidate
                        .strip_prefix(&self.requests_root)
                        .unwrap_or(&candidate)
                        .to_string();

                    return Ok(ResolvedInclude {
                        path: candidate,
                        logical,
                    });
                }
            } else {
                return Err(ResolveError::OutsideRequestsRoot { path: candidate });
            }
        }

        let mut attempted = relative.clone();
        ensure_extension(&mut attempted);
        Err(ResolveError::FileNotFound {
            path: self.requests_root.join(attempted),
        })
    }

    pub fn resolve_env_file(&self, api: &str, env_name: &str) -> Result<Utf8PathBuf, ResolveError> {
        let api_root = self.resolve_api_root(api)?;
        let mut env_relative = sanitize_relative(env_name)?;
        ensure_extension_with(&mut env_relative, "hurlvars");

        let mut candidate_order = Vec::with_capacity(2);

        // Allow explicit path from env_name (no extra directory).
        candidate_order.push(api_root.join(env_relative.as_path()));

        // Also allow `_vars/<env>.hurlvars`.
        let vars_dir = api_root.join(ENVS_DIR_NAME);
        candidate_order.push(vars_dir.join(env_relative.as_path()));

        for candidate in candidate_order {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }

        Err(ResolveError::FileNotFound {
            path: api_root.join(env_relative),
        })
    }

    fn resolve_api_root(&self, api: &str) -> Result<Utf8PathBuf, ResolveError> {
        validate_component(api)?;
        let candidate = self.requests_root.join(api);
        if candidate.is_dir() {
            Ok(candidate)
        } else {
            Err(ResolveError::ApiNotFound {
                api: api.to_string(),
                path: candidate,
            })
        }
    }

    fn resolve_file_path(
        &self,
        api_root: &Utf8Path,
        file: &str,
    ) -> Result<Utf8PathBuf, ResolveError> {
        let mut relative = sanitize_relative(file)?;
        ensure_extension(&mut relative);

        let candidate = api_root.join(relative.as_path());
        if candidate.is_file() {
            Ok(candidate)
        } else {
            Err(ResolveError::FileNotFound { path: candidate })
        }
    }

    fn api_root_for(&self, file_path: &Utf8Path) -> Result<Option<Utf8PathBuf>, ResolveError> {
        if !is_within_root(&self.requests_root, file_path) {
            return Err(ResolveError::OutsideRequestsRoot {
                path: file_path.to_path_buf(),
            });
        }

        let relative = match file_path.strip_prefix(&self.requests_root) {
            Ok(rel) => rel,
            Err(_) => return Ok(None),
        };

        let mut components = relative.components();
        let Some(Utf8Component::Normal(api_name)) = components.next() else {
            return Ok(None);
        };

        let api_root = self.requests_root.join(api_name);
        Ok(Some(api_root))
    }
}

fn sanitize_relative(input: &str) -> Result<Utf8PathBuf, ResolveError> {
    let path = Utf8Path::new(input);

    if path.is_absolute() {
        return Err(ResolveError::InvalidComponent {
            component: input.to_string(),
        });
    }

    for component in path.components() {
        match component {
            Utf8Component::CurDir => continue,
            Utf8Component::Normal(segment) => validate_component(segment)?,
            Utf8Component::ParentDir | Utf8Component::RootDir | Utf8Component::Prefix(_) => {
                return Err(ResolveError::InvalidComponent {
                    component: input.to_string(),
                });
            }
        }
    }

    Ok(path.to_path_buf())
}

fn validate_component(component: &str) -> Result<(), ResolveError> {
    if component.is_empty() {
        return Err(ResolveError::InvalidComponent {
            component: component.to_string(),
        });
    }

    if component.contains(|ch: char| matches!(ch, '\\' | '\0')) {
        return Err(ResolveError::InvalidComponent {
            component: component.to_string(),
        });
    }

    if component == "." || component == ".." {
        return Err(ResolveError::InvalidComponent {
            component: component.to_string(),
        });
    }

    Ok(())
}

fn ensure_extension(path: &mut Utf8PathBuf) {
    if path.extension().is_none() {
        path.set_extension("hurl");
    }
}

fn ensure_extension_with(path: &mut Utf8PathBuf, extension: &str) {
    if path.extension().is_none() {
        path.set_extension(extension);
    }
}

fn is_within_root(root: &Utf8Path, candidate: &Utf8Path) -> bool {
    candidate.starts_with(root)
}
