use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::{env, fs};

/// Returns the current user's home directory, or `None` when it cannot be
/// determined.
///
/// Wraps [` std::env::home_dir `], works correctly cross-platform as of Rust 1.85.0.
pub fn get_user_home_folder() -> Option<PathBuf> {
    env::home_dir()
}

/// Returns the current working directory, falling back to the relative path
/// `.` when it cannot be resolved (for example, when the directory was deleted
/// out from under the process).
pub fn get_current_dir() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// The name of the folder under the user's home that holds every tool's own folder.
pub const TOOLBOX_HOME_FOLDER: &str = ".rusted-toolbox";

/// The toolbox home for `home`: `<home>/.rusted-toolbox`, or `None` when `home` is `None`,
/// empty, or not an absolute path.
pub fn toolbox_home_with(home: Option<PathBuf>) -> Option<PathBuf> {
    home.filter(|home| home.is_absolute())
        .map(|home| home.join(TOOLBOX_HOME_FOLDER))
}

/// The toolbox home, `<home>/.rusted-toolbox`, for the current user's home folder; `None` when
/// the home folder cannot be determined or is not an absolute path.
///
/// A tool's saved data lives under this folder only: with `None`, the data has no default
/// place.
pub fn toolbox_home() -> Option<PathBuf> {
    toolbox_home_with(get_user_home_folder())
}

/// A tool's folder: `<root>/<app_name>`, or `<cwd>/.rusted-toolbox/<app_name>` when there is
/// no `root`.
///
/// `app_name` carries the same no-separator contract as [`get_app_folder`].
pub fn app_folder_in(root: Option<&Path>, cwd: &Path, app_name: &str) -> PathBuf {
    match root {
        Some(root) => root.join(app_name),
        None => cwd.join(TOOLBOX_HOME_FOLDER).join(app_name),
    }
}

/// Returns the application's folder: `app_name`'s folder in the toolbox home
/// (`~/.rusted-toolbox/<app_name>`), or in a `.rusted-toolbox` folder in the current
/// directory when the home folder cannot be determined. Log files use it; a tool's saved data
/// takes its place from [`toolbox_home`] instead, which has no current-directory fallback.
///
/// `app_name` must be a plain name with no path separators: a package name
/// (`env!("CARGO_PKG_NAME")` cannot contain separators) or another fixed name
/// from the code, never text from outside the program. The value is joined
/// under the toolbox home and reaches `create_dir_all`, so this contract is the
/// guard against path traversal.
///
/// Note that this is the app DATA folder, not where the executable is (necessarily) located.
pub fn get_app_folder(app_name: &str) -> PathBuf {
    app_folder_in(toolbox_home().as_deref(), &get_current_dir(), app_name)
}

/// Returns a named subfolder within the application folder for `app_name`.
///
/// `app_name` carries the same no-separator contract as [`get_app_folder`],
/// and `sub_folder` is a fixed name from the code as well.
pub fn get_app_sub_folder(app_name: &str, sub_folder: &str) -> PathBuf {
    get_app_folder(app_name).join(sub_folder)
}

/// Builds a filename of the form `prefix-<timestamp>.ext`.
///
/// The timestamp is `%Y-%m-%d` when `use_date_only` is set, otherwise
/// `%Y-%m-%d_%H-%M-%S-%3f`. `use_utc` selects UTC over local time.
pub fn get_filename_with_current_date(
    prefix: &str,
    ext: &str,
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

    format!("{prefix}-{now}.{ext}")
}

pub trait EnsureDirectoryExists {
    /// Creates the directory at this path, including any missing parents, when
    /// it does not already exist.
    ///
    /// # Errors
    /// Returns an error when the directory cannot be created.
    fn ensure_directory_exists(&self) -> Result<()>;

    /// Creates this path's parent directory, including any missing ancestors,
    /// when it does not already exist.
    ///
    /// # Errors
    /// Returns an error when the path has no parent, or the parent cannot be
    /// created.
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

/// Resolves `file` to an absolute path: returned unchanged when already
/// absolute, otherwise joined onto the current working directory.
pub fn get_full_filepath_from_string(file: &str) -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbox_home_with_an_absolute_home_is_the_hidden_folder_in_it() {
        let home = tempfile::tempdir().unwrap();

        let root = toolbox_home_with(Some(home.path().to_path_buf()));

        assert_eq!(root, Some(home.path().join(".rusted-toolbox")));
    }

    #[rstest::rstest]
    #[case::no_home(None)]
    #[case::empty_home(Some(PathBuf::new()))]
    #[case::relative_home(Some(PathBuf::from("relative/home")))]
    fn toolbox_home_with_no_usable_home_is_none(#[case] home: Option<PathBuf>) {
        assert_eq!(toolbox_home_with(home), None);
    }

    #[test]
    fn app_folder_in_a_root_is_the_app_name_under_it() {
        let root = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();

        let folder = app_folder_in(Some(root.path()), cwd.path(), "b64");

        assert_eq!(folder, root.path().join("b64"));
    }

    #[test]
    fn app_folder_in_with_no_root_is_the_toolbox_folder_in_the_current_folder() {
        let cwd = tempfile::tempdir().unwrap();

        let folder = app_folder_in(None, cwd.path(), "b64");

        assert_eq!(folder, cwd.path().join(".rusted-toolbox").join("b64"));
    }

    #[test]
    fn app_sub_folder_appends_sub_folder_component() {
        let folder = get_app_sub_folder("b64", "logs");
        assert!(folder.ends_with(".rusted-toolbox/b64/logs"));
    }

    #[test]
    fn filename_with_current_date_wraps_prefix_and_extension() {
        let name = get_filename_with_current_date("app", "log", true, true);
        assert!(name.starts_with("app-"));
        assert!(name.ends_with(".log"));
    }

    #[test]
    fn ensure_directory_exists_creates_missing_nested_directory() {
        let temp = tempfile::tempdir().unwrap();
        let nested = temp.path().join("a").join("b").join("c");
        assert!(!nested.exists());

        nested.ensure_directory_exists().unwrap();

        assert!(nested.exists());
    }

    #[test]
    fn ensure_parent_exists_creates_parent_of_file() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("x").join("y").join("file.txt");
        let parent = file.parent().unwrap().to_path_buf();
        assert!(!parent.exists());

        file.ensure_parent_exists().unwrap();

        assert!(parent.exists());
    }

    #[test]
    fn full_filepath_keeps_absolute_input() {
        let temp = tempfile::tempdir().unwrap();
        let absolute = temp.path().join("file.txt");
        assert!(absolute.is_absolute());

        let resolved = get_full_filepath_from_string(absolute.to_str().unwrap());

        assert_eq!(resolved, absolute);
    }

    #[test]
    fn full_filepath_joins_relative_onto_cwd() {
        let resolved = get_full_filepath_from_string("some_relative_file.txt");

        assert!(resolved.is_absolute());
        assert!(resolved.ends_with("some_relative_file.txt"));
    }

    #[test]
    fn resolve_path_with_base_prefers_absolute() {
        let temp = tempfile::tempdir().unwrap();
        let absolute = temp.path().join("file.txt");

        let resolved = resolve_path_with_base("base_dir", absolute.to_str().unwrap());

        assert_eq!(resolved, absolute);
    }

    #[test]
    fn resolve_path_with_base_joins_relative() {
        let resolved = resolve_path_with_base("base_dir", "sub/file.txt");

        assert_eq!(resolved, PathBuf::from("base_dir").join("sub/file.txt"));
    }
}
