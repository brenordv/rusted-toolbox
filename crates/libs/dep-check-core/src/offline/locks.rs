//! The advisory file locks around the offline stores and settings. A store's lock is taken
//! without waiting: a lock another holder has is an answer (`Held`), never a wait. The lock
//! files are created when first needed and never deleted: on Unix, deleting a held lock file
//! lets a second process lock a new file at the same path.

use common_file_utils::permissions::open_owner_only;
use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::Path;

/// Why a lock was not taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LockFailure {
    /// Another holder has it.
    Held,
    /// The lock file could not be opened or locked; the cleaned reason.
    Failed(String),
}

/// `path` locked for this holder alone, for as long as the returned file is open.
pub(crate) fn try_exclusive(path: &Path) -> Result<File, LockFailure> {
    let file = open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(error) => Err(failure(error)),
    }
}

/// `path` locked beside other shared holders, for as long as the returned file is open.
pub(crate) fn try_shared(path: &Path) -> Result<File, LockFailure> {
    let file = open(path)?;
    match file.try_lock_shared() {
        Ok(()) => Ok(file),
        Err(error) => Err(failure(error)),
    }
}

/// `path` locked for this holder alone, waiting for another holder to let go: only for a lock
/// every holder keeps for a few file operations, as a settings change does.
pub(crate) fn lock_waiting(path: &Path) -> io::Result<File> {
    let file = open_file(path)?;
    file.lock()?;
    Ok(file)
}

fn open(path: &Path) -> Result<File, LockFailure> {
    open_file(path).map_err(|error| LockFailure::Failed(super::cleaned(&error.to_string())))
}

fn open_file(path: &Path) -> io::Result<File> {
    open_owner_only(
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false),
        path,
    )
}

fn failure(error: TryLockError) -> LockFailure {
    match error {
        TryLockError::WouldBlock => LockFailure::Held,
        TryLockError::Error(error) => LockFailure::Failed(super::cleaned(&error.to_string())),
    }
}
