//! The offline-data settings, `~/.rusted-toolbox/dep-check/offline.json`, and the choice of how
//! one check uses offline data.

use crate::offline::locks;
use crate::offline::store::DataStatus;
use anyhow::{Context, Result, bail};
use chrono::TimeDelta;
use common_file_utils::permissions::create_dir_all_owner_only;
use common_serialization_utils::save_object_to_json_file;
use common_utils::file_system::toolbox_home;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// The settings file's shape; a file with a higher number is never written.
pub const SETTINGS_VERSION: u32 = 1;
/// How long a no-finding answer from offline data counts in `Fallback`, from when the data was
/// last confirmed current.
pub const FALLBACK_MAX_AGE: TimeDelta = TimeDelta::days(7);
/// The same for `Only`, where no service checks the answer.
pub const ONLY_MAX_AGE: TimeDelta = TimeDelta::hours(48);

/// Whether and how offline data is used, and whether it updates on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Offline data is used at all. Off keeps the data on disk.
    pub enabled: bool,
    pub mode: OfflineMode,
    /// A downloaded ecosystem is updated when its data is a day old.
    pub auto_update: bool,
}

impl Default for Settings {
    /// On, as a fallback, updated on request.
    fn default() -> Self {
        Self {
            enabled: true,
            mode: OfflineMode::Fallback,
            auto_update: false,
        }
    }
}

/// When offline data answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfflineMode {
    /// For the packages OSV.dev did not answer.
    Fallback,
    /// Instead of the services, for every ecosystem with downloaded data.
    Always,
}

impl OfflineMode {
    /// The mode's word, as the file and log lines write it.
    pub fn name(self) -> &'static str {
        match self {
            OfflineMode::Fallback => "fallback",
            OfflineMode::Always => "always",
        }
    }
}

/// One change to the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingChange {
    Enabled(bool),
    Mode(OfflineMode),
    AutoUpdate(bool),
}

/// How one check uses offline data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OfflineUse {
    /// Not at all.
    Off,
    /// For the packages OSV.dev did not answer.
    Fallback,
    /// Instead of every service: no package name leaves the machine.
    Only,
}

impl OfflineUse {
    /// The use for one check of an ecosystem whose store reads `status`. A one-run force gives
    /// `Only` whatever the settings say. Otherwise: `Off` when offline data is turned off;
    /// `Fallback` in fallback mode; in always mode `Off` only when the ecosystem has no data at
    /// all, and `Only` for every other status, so a store that cannot be used right now fails
    /// the check closed instead of sending the names to the services.
    pub fn decide(settings: &Settings, status: &DataStatus, force_offline: bool) -> OfflineUse {
        if force_offline {
            return OfflineUse::Only;
        }
        if !settings.enabled {
            return OfflineUse::Off;
        }
        match settings.mode {
            OfflineMode::Fallback => OfflineUse::Fallback,
            OfflineMode::Always => match status {
                DataStatus::NotDownloaded => OfflineUse::Off,
                DataStatus::Ready { .. }
                | DataStatus::Unavailable { .. }
                | DataStatus::Locked
                | DataStatus::Unreadable { .. }
                | DataStatus::NewerFormat => OfflineUse::Only,
            },
        }
    }

    /// How old offline data may be, since it was last confirmed current, for a no-finding
    /// answer from it to count: 7 days in `Fallback`, 48 hours in `Only`. `Off` reads no
    /// offline data, so its window is empty.
    pub fn max_age(self) -> TimeDelta {
        match self {
            OfflineUse::Off => TimeDelta::zero(),
            OfflineUse::Fallback => FALLBACK_MAX_AGE,
            OfflineUse::Only => ONLY_MAX_AGE,
        }
    }

    /// The use's word, as the report and log lines write it.
    pub fn name(self) -> &'static str {
        match self {
            OfflineUse::Off => "off",
            OfflineUse::Fallback => "fallback",
            OfflineUse::Only => "only",
        }
    }
}

/// The settings file in the toolbox home, `~/.rusted-toolbox/dep-check/offline.json`; `None`
/// when there is no home folder.
pub fn settings_path() -> Option<PathBuf> {
    toolbox_home().map(|home| home.join("dep-check").join("offline.json"))
}

/// The file as this version writes it. Every field but the version has a default, so a file
/// missing one still reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SettingsFile {
    settings_version: u32,
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default = "default_mode")]
    mode: OfflineMode,
    #[serde(default)]
    auto_update: bool,
}

fn default_enabled() -> bool {
    Settings::default().enabled
}

fn default_mode() -> OfflineMode {
    Settings::default().mode
}

impl SettingsFile {
    fn of(settings: Settings) -> Self {
        Self {
            settings_version: SETTINGS_VERSION,
            enabled: settings.enabled,
            mode: settings.mode,
            auto_update: settings.auto_update,
        }
    }

    fn settings(&self) -> Settings {
        Settings {
            enabled: self.enabled,
            mode: self.mode,
            auto_update: self.auto_update,
        }
    }
}

/// What the file at a path holds.
enum OnDisk {
    Missing,
    Read(Settings),
    /// Saved by a newer dep-check: read for the fields this version knows, never written.
    Newer(Settings),
    /// Content that does not parse; a change replaces it.
    Unparseable(anyhow::Error),
}

fn read(path: &Path) -> Result<OnDisk> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            return Ok(OnDisk::Missing);
        }
        Err(error) => return Err(error).context("failed to read the offline settings"),
    };
    let unparseable = |error: serde_json::Error| {
        OnDisk::Unparseable(anyhow::Error::new(error).context("the offline settings do not parse"))
    };
    let value: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(error) => return Ok(unparseable(error)),
    };
    // A version this one cannot read as a number counts as newer, so it is never overwritten.
    let newer = value.get("settings_version").is_some_and(|version| {
        version
            .as_u64()
            .is_none_or(|version| version > u64::from(SETTINGS_VERSION))
    });
    if newer {
        return Ok(OnDisk::Newer(known_fields(&value)));
    }
    Ok(match SettingsFile::deserialize(&value) {
        Ok(file) => OnDisk::Read(file.settings()),
        Err(error) => unparseable(error),
    })
}

/// The fields this version knows, from a file a newer dep-check wrote. Each one reads as this
/// version reads it, and a missing or unreadable one takes its default, except `mode`: a mode
/// this version does not know reads as `Always`, so no package name leaves the machine because
/// an older tool could not read a newer choice.
fn known_fields(value: &Value) -> Settings {
    let defaults = Settings::default();
    Settings {
        enabled: value
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(defaults.enabled),
        mode: match value.get("mode").map(OfflineMode::deserialize) {
            None => defaults.mode,
            Some(Ok(mode)) => mode,
            Some(Err(_)) => OfflineMode::Always,
        },
        auto_update: value
            .get("auto_update")
            .and_then(Value::as_bool)
            .unwrap_or(defaults.auto_update),
    }
}

/// The settings at `path`. No path or no file gives the defaults, and a file a newer dep-check
/// saved gives the fields this version knows. A file that cannot be read or parsed is logged at
/// warn and gives the defaults, and the next change replaces it.
pub fn load(path: Option<&Path>) -> Settings {
    let Some(path) = path else {
        return Settings::default();
    };
    match read(path) {
        Ok(OnDisk::Read(settings) | OnDisk::Newer(settings)) => settings,
        Ok(OnDisk::Missing) => Settings::default(),
        Ok(OnDisk::Unparseable(error)) | Err(error) => {
            // The settings fall back to their defaults: offline data on, as a fallback, which
            // is also what a first run sees.
            warn!(path = %path.display(), error = %format!("{error:#}"), "the offline settings could not be read");
            Settings::default()
        }
    }
}

/// The lock beside the settings file that a change holds from its read to its write.
fn lock_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".lock");
    path.with_file_name(name)
}

/// Re-reads the file at `path`, applies `change`, and writes the result atomically, all while
/// holding a lock beside the file, so two writers never put back each other's old values.
/// Returns the settings written.
///
/// # Errors
/// No path (no home folder); a lock or a file that cannot be read for a reason other than its
/// content; a file saved by a newer dep-check, which is never written; a failed write.
pub fn change(path: Option<&Path>, change: SettingChange) -> Result<Settings> {
    let Some(path) = path else {
        bail!("there is no home folder to keep the offline settings in");
    };
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        create_dir_all_owner_only(parent).context("failed to create the settings folder")?;
    }
    let _writing =
        locks::lock_waiting(&lock_path(path)).context("failed to lock the offline settings")?;
    let mut settings = match read(path)? {
        OnDisk::Newer(_) => bail!("these settings were saved by a newer dep-check"),
        OnDisk::Read(settings) => settings,
        OnDisk::Missing | OnDisk::Unparseable(_) => Settings::default(),
    };
    match change {
        SettingChange::Enabled(enabled) => settings.enabled = enabled,
        SettingChange::Mode(mode) => settings.mode = mode,
        SettingChange::AutoUpdate(auto_update) => settings.auto_update = auto_update,
    }
    save_object_to_json_file(path, &SettingsFile::of(settings)).inspect_err(|error| {
        warn!(path = %path.display(), error = %format!("{error:#}"), "the offline settings could not be written");
    })?;
    info!(
        enabled = settings.enabled,
        mode = settings.mode.name(),
        auto_update = settings.auto_update,
        "offline settings changed"
    );
    Ok(settings)
}

#[cfg(test)]
mod tests;
