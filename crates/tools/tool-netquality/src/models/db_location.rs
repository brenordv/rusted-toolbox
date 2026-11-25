//! Where netquality's database is when no path is chosen, and moving it there from the folder
//! of the executable, where it was before netquality 3.0.0.

use super::types::StorageConfig;
use anyhow::{Context, Result};
use common_file_utils::adoption::{Adoption, Shape, adopt, adopt_in_use};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// The database's file name, in netquality's folder and beside the executable.
pub const DB_FILE: &str = "netquality.db";
/// netquality's folder in the toolbox home: its package name, like every tool's.
const DB_FOLDER: &str = env!("CARGO_PKG_NAME");
/// What the database is called in the line that reports moving it.
const DB_DATA: &str = "netquality's database";
/// The files SQLite keeps beside a database, under the database's name, while it is in use.
const JOURNAL_SUFFIXES: [&str; 3] = ["-journal", "-wal", "-shm"];

/// The database path when neither `--db-path` nor `db_path` is set, with the old default to
/// move from: `<root>/netquality/netquality.db` and `<exe folder>/netquality.db`. With no
/// toolbox home `root`, the old default itself, with nothing to move.
///
/// # Errors
/// When `exe_folder` fails: the old default cannot be found.
pub fn default_db_path(
    root: Option<PathBuf>,
    exe_folder: impl FnOnce() -> Result<PathBuf>,
) -> Result<(PathBuf, Option<PathBuf>)> {
    let old_default = exe_folder()
        .context("Failed to get tool path")?
        .join(DB_FILE);
    Ok(match root {
        Some(root) => (root.join(DB_FOLDER).join(DB_FILE), Some(old_default)),
        None => (old_default, None),
    })
}

/// What became of a database at the old default.
#[derive(Debug)]
pub struct DbAdoption {
    target: PathBuf,
    adoption: Adoption,
}

impl DbAdoption {
    /// Logs a database moved from the old default, or left there, at `warn`.
    pub fn report(&self) {
        self.adoption.report(DB_DATA, &self.target);
    }
}

impl StorageConfig {
    /// Moves the database at the old default ([`StorageConfig::legacy_db`]) to `db_path` when
    /// only the old one exists, and points `db_path` at the database to use. Nothing moves or
    /// is removed while an SQLite journal file sits beside either database, or its presence
    /// cannot be checked: SQLite keeps a journal beside a database under the database's name,
    /// and a database moved away from its journal can be corrupted. Returns what happened;
    /// `None` when there is no old default (a path was chosen, or there is no home folder).
    pub fn adopt_legacy_db(&mut self) -> Option<DbAdoption> {
        let legacy = self.legacy_db.take()?;
        let target = self.db_path.clone();
        let adoption = match journal_beside(&legacy).or_else(|| journal_beside(&target)) {
            Some(journal) => adopt_in_use(
                Some(&legacy),
                &target,
                Shape::Database,
                &format!("{} is there or cannot be checked", journal.display()),
            ),
            None => adopt(Some(&legacy), &target, Shape::Database),
        };
        self.db_path = adoption.path_in_use(&target);
        Some(DbAdoption { target, adoption })
    }
}

/// The first SQLite journal file beside `db`, or the first one whose presence cannot be
/// checked. A journal path under a regular file cannot exist and counts as absent.
fn journal_beside(db: &Path) -> Option<PathBuf> {
    JOURNAL_SUFFIXES
        .iter()
        .map(|suffix| {
            let mut name = db.as_os_str().to_owned();
            name.push(suffix);
            PathBuf::from(name)
        })
        .find(|journal| match fs::symlink_metadata(journal) {
            Err(error)
                if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) =>
            {
                false
            }
            Ok(_) | Err(_) => true,
        })
}

#[cfg(test)]
mod tests;
