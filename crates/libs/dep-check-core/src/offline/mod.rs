//! Offline OSV data: OSV.dev's per-ecosystem dumps, downloaded on request into one SQLite store
//! per ecosystem, and checked against locally when the services cannot be asked, or instead of
//! them.
//!
//! - [`settings`]: whether and how offline data is used, and [`OfflineUse`] for one check.
//! - [`store`]: the stores, their status, and their removal.
//! - [`update`]: the download and the install, with progress.
//! - [`due`]: which downloaded ecosystems a daily update should refresh.

use common_utils::file_system::toolbox_home;
use common_utils::string_utils::{display_line, error_preview};
use std::path::PathBuf;

pub mod due;
#[cfg(test)]
mod live_tests;
pub(crate) mod locks;
pub(crate) mod matching;
pub mod settings;
pub mod store;
#[cfg(test)]
pub(crate) mod testing;
pub mod update;
pub(crate) mod versions;

pub use settings::{OfflineMode, OfflineUse, SettingChange, Settings};
pub use store::{DataStatus, FailedAttempt, RemoveOutcome, StoreMeta};

/// The most characters of a record id kept in a failure's words.
const ID_CHARS: usize = 64;

/// The folder the stores live in, `~/.rusted-toolbox/dep-check/offline`; `None` when there is no
/// home folder.
pub fn offline_dir() -> Option<PathBuf> {
    toolbox_home().map(|home| home.join("dep-check").join("offline"))
}

/// `text` on one line and cut, for a reason that came from outside the code (an error's words,
/// text read back from a store).
pub(crate) fn cleaned(text: &str) -> String {
    error_preview(text).shown
}

/// A record id on one line and cut to 64 characters: the one piece of data text a failure's
/// words may hold.
pub(crate) fn cleaned_id(id: &str) -> String {
    display_line(id.chars().take(ID_CHARS))
}
