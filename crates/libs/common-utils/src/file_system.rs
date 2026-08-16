use anyhow::{Context, Result};
use std::{env, fs};
use std::path::{Path, PathBuf};

/// Wrapper created because historically Windows had a bug that required some workaround,
/// and I need to make any changes, having a wrapper from the get-go is easier.
pub fn get_user_home_folder() -> Option<PathBuf> {
    env::home_dir()
}

pub fn get_current_dir() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn get_app_folder(app_name: &str) -> PathBuf {
    let app_folder = format!(".{app_name}");
    match get_user_home_folder() {
        Some(user_home) => user_home.join(app_folder),
        None => get_current_dir().join(app_folder),
    }
}

pub fn get_app_sub_folder(app_name: &str, sub_folder: String) -> PathBuf {
    get_app_folder(app_name).join(sub_folder)
}

pub fn get_filename_with_current_date(
    prefix: String,
    ext: String,
    use_utc: bool,
    use_date_only: bool,
) -> String {
    let now_format = if use_date_only {
        "%Y-%m-%d"
    } else {
        "%Y-%m-%d_%H-%M-%S-%3f"
    };

    let now = if use_utc {
        chrono::Utc::now().format(now_format)
    } else {
        chrono::Local::now().format(now_format)
    };

    format!("{}-{}.{}", prefix, now, ext)
}

pub trait EnsureDirectoryExists {
    fn ensure_directory_exists(&self) -> Result<()>;
    fn ensure_parent_exists(&self) -> Result<()>;
}

impl EnsureDirectoryExists for PathBuf {
    fn ensure_directory_exists(&self) -> Result<()> {
        if self.exists() {
            return Ok(());
        }

        fs::create_dir_all(self).context("Failed to create directory")
    }

    fn ensure_parent_exists(&self) -> Result<()> {
        let parent = self.parent().context("Failed to get parent directory")?;

        if parent.exists() {
            return Ok(());
        }

        fs::create_dir_all(parent).context("Failed to create directory")
    }
}

pub fn get_full_filepath_from_string(file: &String) -> PathBuf {
    let path = Path::new(file);
    if path.is_absolute() {
        // If it's already an absolute path, convert it to PathBuf and return
        path.to_path_buf()
    } else {
        // If it's a relative path, join it with the current working directory
        let cwd = get_current_dir();
        cwd.join(path)
    }
}

/// Resolves a given path against a base folder.
///
/// This function takes a base folder and a relative or absolute path as input.
/// If the provided path is absolute, it returns the path as is. However, if the
/// path is relative, it resolves the path by appending it to the provided base folder.
///
/// # Arguments
///
/// * `base_folder` - A string slice representing the base folder to resolve the path against.
/// * `path` - A string slice representing the path which may be absolute or relative.
///
/// # Returns
///
/// A `PathBuf` representing the resolved path. If the provided path is absolute, the result
/// is the same as the input path. If the path is relative, the result is the base folder
/// joined with the relative path.
pub fn resolve_path_with_base(base_folder: &str, path: &str) -> PathBuf {
    let path_buf = PathBuf::from(path);
    if path_buf.is_absolute() {
        path_buf
    } else {
        PathBuf::from(base_folder).join(path)
    }
}