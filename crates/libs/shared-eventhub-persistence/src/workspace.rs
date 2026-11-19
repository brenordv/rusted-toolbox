//! The workspace: one folder holding saved connection profiles, one
//! subfolder each. A profile folder looks like any working folder: a
//! `config.json` and, beside it, the data folder that config names.

use crate::config::{ConfigLoad, ToolConfig, read_config};
use crate::paths::DataPaths;
use anyhow::{Context, bail};
use common_utils::file_system::get_user_home_folder;
use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Environment variable naming the workspace folder (an absolute path).
pub const WORKSPACE_ENV: &str = "EH_WORKSPACE";

/// eh's per-user folder under the home folder.
const EH_HOME_FOLDER: &str = ".eh";
/// The workspace folder inside [`EH_HOME_FOLDER`].
const PROFILES_FOLDER: &str = "profiles";
/// The longest profile name, in characters.
const MAX_NAME_LENGTH: usize = 64;
/// Windows device names. A file name whose part before the first `.` is one
/// of these opens the device, so `nul.txt` is `NUL`.
const DEVICE_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];
const NOT_A_PROFILE_NAME: &str = "the folder name is not a valid profile name: rename it";

/// A profile name that is safe as a folder name on Windows, macOS, and
/// Linux: 1 to 64 ASCII letters, digits, `-`, `_`, and `.`, starting with a
/// letter or digit, not ending with `.`, and not a Windows device name. It
/// holds no separator, `..`, or drive prefix, so joining it under a folder
/// stays in that folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileName(String);

impl ProfileName {
    /// Checks `text` against the name rules.
    ///
    /// # Errors
    /// Fails when `text` breaks a rule, naming the rule. The message quotes
    /// `text` only when every character is allowed.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let bytes = text.as_bytes();
        let well_formed = (1..=MAX_NAME_LENGTH).contains(&bytes.len())
            && bytes
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
            && !text.ends_with('.');
        if !well_formed {
            bail!(
                "A profile name uses letters, digits, '-', '_', and '.', starts with a letter or \
                 digit, does not end with '.', and has at most {MAX_NAME_LENGTH} characters"
            );
        }
        let stem = text.split('.').next().unwrap_or(text);
        if DEVICE_NAMES
            .iter()
            .any(|device| device.eq_ignore_ascii_case(stem))
        {
            bail!("{text} is a device name on Windows; pick another name");
        }
        Ok(Self(text.to_owned()))
    }

    /// The name as given.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One profile folder in the workspace, whether or not it exists yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    name: String,
    dir: PathBuf,
}

impl Profile {
    /// The name as the folder spells it, or as given for a folder that does
    /// not exist yet.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The profile folder: `<workspace>/<name>`.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// What listing found in one profile folder.
#[derive(Clone, Debug)]
pub enum ProfileState {
    Ready(Box<ToolConfig>),
    /// The folder holds an eh `config.json` that cannot be used; the text
    /// says why.
    CannotLoad(String),
}

impl ProfileState {
    fn is_ready(&self) -> bool {
        match self {
            Self::Ready(_) => true,
            Self::CannotLoad(_) => false,
        }
    }
}

/// A profile folder and what its `config.json` holds.
#[derive(Clone, Debug)]
pub struct ProfileEntry {
    /// The folder, named as it is on disk.
    pub profile: Profile,
    /// Whether its `config.json` can be used.
    pub state: ProfileState,
}

/// The folder that holds the profiles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    root: PathBuf,
}

/// A direct subfolder of the workspace root.
struct Subfolder {
    name: OsString,
    path: PathBuf,
}

impl Workspace {
    /// The workspace at [`WORKSPACE_ENV`], or `<home>/.eh/profiles` when the
    /// variable is unset or empty.
    ///
    /// # Errors
    /// Fails when the variable is set to a path that is not absolute, or
    /// when it is unset and there is no home folder.
    pub fn locate() -> anyhow::Result<Self> {
        locate_with(std::env::var_os(WORKSPACE_ENV), get_user_home_folder())
    }

    /// The workspace whose profile folders sit directly under `root`.
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// The folder that holds the profile folders.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The folder a profile named `name` has or would have: an existing
    /// folder whose name matches ignoring ASCII case, else `<root>/<name>`.
    ///
    /// # Errors
    /// Fails when the root exists but cannot be listed, or when two folders
    /// match `name` ignoring case.
    pub async fn profile(&self, name: &ProfileName) -> anyhow::Result<Profile> {
        let mut matching: Vec<Subfolder> = self
            .subfolders()
            .await?
            .into_iter()
            .filter(|folder| {
                folder
                    .name
                    .to_str()
                    .is_some_and(|folder_name| folder_name.eq_ignore_ascii_case(name.as_str()))
            })
            .collect();
        matching.sort_by(|first, second| first.name.cmp(&second.name));
        match matching.as_slice() {
            [] => Ok(Profile {
                name: name.as_str().to_owned(),
                dir: self.root.join(name.as_str()),
            }),
            [folder] => Ok(Profile {
                name: folder.name.to_string_lossy().into_owned(),
                dir: folder.path.clone(),
            }),
            [first, second, ..] => bail!(
                "Two profile folders match {} ignoring case: '{}' and '{}'; rename one",
                name.as_str(),
                first.name.to_string_lossy(),
                second.name.to_string_lossy()
            ),
        }
    }

    /// Every profile folder: a subfolder holding an eh `config.json`,
    /// including one whose name or config cannot be used. Sorted by name
    /// ignoring ASCII case. A missing root lists nothing. Nothing is logged
    /// per profile.
    ///
    /// # Errors
    /// Fails when the root exists but cannot be listed.
    pub async fn list(&self) -> anyhow::Result<Vec<ProfileEntry>> {
        let mut entries = Vec::new();
        for folder in self.subfolders().await? {
            let valid_name = folder
                .name
                .to_str()
                .is_some_and(|text| ProfileName::parse(text).is_ok());
            let state = match (read_config(&folder.path).await, valid_name) {
                (Ok(ConfigLoad::Missing | ConfigLoad::Foreign), _) => {
                    debug!(
                        path = %folder.path.display(),
                        "Skipped a workspace folder without an eh config.json"
                    );
                    continue;
                }
                (Ok(ConfigLoad::Loaded(_)) | Err(_), false) => {
                    ProfileState::CannotLoad(NOT_A_PROFILE_NAME.to_owned())
                }
                (Ok(ConfigLoad::Loaded(config)), true) => ProfileState::Ready(config),
                (Err(error), true) => ProfileState::CannotLoad(format!("{error:#}")),
            };
            entries.push(ProfileEntry {
                profile: Profile {
                    name: folder.name.to_string_lossy().into_owned(),
                    dir: folder.path,
                },
                state,
            });
        }
        entries.sort_by_cached_key(|entry| {
            (
                entry.profile.name.to_ascii_lowercase(),
                entry.profile.name.clone(),
            )
        });
        let ready = entries
            .iter()
            .filter(|entry| entry.state.is_ready())
            .count();
        debug!(
            ready,
            cannot_load = entries.len() - ready,
            "Listed profiles"
        );
        Ok(entries)
    }

    /// The root's direct subfolders, links followed. A missing root has
    /// none.
    async fn subfolders(&self) -> anyhow::Result<Vec<Subfolder>> {
        let listing_error = || format!("Failed to list the workspace '{}'", self.root.display());
        let mut entries = match tokio::fs::read_dir(&self.root).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(anyhow::Error::new(error).context(listing_error())),
        };
        let mut folders = Vec::new();
        while let Some(entry) = entries.next_entry().await.with_context(listing_error)? {
            let path = entry.path();
            // An entry that is not a folder, or that cannot be inspected (a
            // link to nowhere), holds no profile; one such entry must not
            // hide the others.
            match tokio::fs::metadata(&path).await {
                Ok(metadata) if metadata.is_dir() => folders.push(Subfolder {
                    name: entry.file_name(),
                    path,
                }),
                Ok(_) => debug!(
                    path = %path.display(),
                    "Skipped a workspace entry that is not a folder"
                ),
                Err(error) => debug!(
                    path = %path.display(),
                    error = %error,
                    "Skipped a workspace entry that cannot be inspected"
                ),
            }
        }
        Ok(folders)
    }
}

/// The workspace `variable` (the value of [`WORKSPACE_ENV`]) names, or the
/// default one under `home`.
fn locate_with(variable: Option<OsString>, home: Option<PathBuf>) -> anyhow::Result<Workspace> {
    match variable.filter(|value| !value.is_empty()) {
        Some(value) => {
            let root = PathBuf::from(value);
            if !root.is_absolute() {
                bail!(
                    "{WORKSPACE_ENV} must be an absolute folder path; it is '{}'",
                    root.display()
                );
            }
            Ok(Workspace::at(root))
        }
        None => match home {
            Some(home) => Ok(Workspace::at(
                home.join(EH_HOME_FOLDER).join(PROFILES_FOLDER),
            )),
            None => bail!(
                "No home folder was found to keep profiles in; set {WORKSPACE_ENV} to an \
                 absolute folder path"
            ),
        },
    }
}

/// The folder a run's `config.json` is read from and saved to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigHome {
    /// The folder the tool was started in.
    CurrentFolder(PathBuf),
    Profile(Profile),
}

impl ConfigHome {
    /// The folder `config.json` is read from and saved to.
    pub fn dir(&self) -> &Path {
        match self {
            Self::CurrentFolder(dir) => dir,
            Self::Profile(profile) => profile.dir(),
        }
    }

    /// The profile's name; `None` for the current folder.
    pub fn profile_name(&self) -> Option<&str> {
        match self {
            Self::CurrentFolder(_) => None,
            Self::Profile(profile) => Some(profile.name()),
        }
    }

    /// The data folder `base_data_folder` names from this home. Under a
    /// profile, a relative folder sits inside the profile folder; under the
    /// current folder it stays relative, so it resolves against the folder
    /// the tool was started in. An absolute folder is kept as given.
    pub fn data_paths(&self, base_data_folder: &str) -> DataPaths {
        match self {
            Self::CurrentFolder(_) => DataPaths::new(base_data_folder),
            Self::Profile(profile) => DataPaths::new(profile.dir().join(base_data_folder)),
        }
    }
}

#[cfg(test)]
mod tests;
