//! Moving a tool's data from its old default place to its new one, once.
//!
//! [`adopt`] looks at both paths, moves the data when only the old place holds it, and says
//! what it found in an [`Adoption`]. It never copies, never overwrites the new path, and never
//! removes a symbolic link. When both places hold data it decides by file identity, never by
//! contents, since data still in use (a database, a settings file) changes.

use crate::permissions::create_dir_all_owner_only;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tracing::warn;

/// What the data at a path is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// One file, moved by a hard link at the new path and then removing the old name.
    File,
    /// A folder and everything in it, moved by one rename.
    Folder,
}

/// Why data stayed in its old place.
#[derive(Debug)]
pub enum KeptReason {
    /// The old path is a symbolic link; moving a link can change what it points at.
    SymbolicLink,
    /// A step of the move failed: the new path's folder could not be created, the data could
    /// not be linked or renamed there (another filesystem, a file system with no hard links),
    /// or the old name could not be removed (another process may still have it open).
    Failed(io::Error),
}

/// What [`adopt`] found and did.
#[derive(Debug)]
pub enum Adoption {
    /// Neither place holds data; the new path is created on the first write.
    Nothing,
    /// The data is at the new path already, and the old place holds none of its own.
    AlreadyThere,
    /// The data moved from `from` to the new path.
    Moved {
        /// Where the data was.
        from: PathBuf,
    },
    /// Both places hold data. The new path is used; `legacy` is not read.
    BothPresent {
        /// The old place, left as it is.
        legacy: PathBuf,
    },
    /// The data stays at `legacy`, which is used this run.
    KeptLegacy {
        /// The old place, still in use.
        legacy: PathBuf,
        /// Why the data did not move.
        reason: KeptReason,
    },
}

impl Adoption {
    /// The path to read and write the data at: the old place for [`Adoption::KeptLegacy`],
    /// `target` for every other outcome.
    pub fn path_in_use(&self, target: &Path) -> PathBuf {
        match self {
            Adoption::KeptLegacy { legacy, .. } => legacy.clone(),
            Adoption::Nothing
            | Adoption::AlreadyThere
            | Adoption::Moved { .. }
            | Adoption::BothPresent { .. } => target.to_path_buf(),
        }
    }

    /// Logs the outcome at `warn`, naming `data` (what the data is, such as "speak's
    /// settings") and both paths: a move, data found in both places, and data kept in the old
    /// place, each with what to do about it. [`Adoption::Nothing`] and
    /// [`Adoption::AlreadyThere`] log nothing.
    pub fn report(&self, data: &str, target: &Path) {
        match self {
            Adoption::Nothing | Adoption::AlreadyThere => {}
            Adoption::Moved { from } => warn!(
                data,
                old = %from.display(),
                new = %target.display(),
                "moved to the toolbox folder; the old folder can be deleted once it is empty"
            ),
            Adoption::BothPresent { legacy } => warn!(
                data,
                old = %legacy.display(),
                new = %target.display(),
                "found in the old place and the new one; the old one is not read: copy what \
                 you need from it, then delete it"
            ),
            Adoption::KeptLegacy {
                legacy,
                reason: KeptReason::SymbolicLink,
            } => warn!(
                data,
                old = %legacy.display(),
                new = %target.display(),
                "the old place is a symbolic link, so it stays in use; create a link at the new \
                 path pointing where this one does to stop this message"
            ),
            Adoption::KeptLegacy {
                legacy,
                reason: KeptReason::Failed(error),
            } => warn!(
                data,
                old = %legacy.display(),
                new = %target.display(),
                error = %error,
                "could not be moved, so the old place stays in use; move it there by hand to \
                 stop this message"
            ),
        }
    }
}

/// Moves the data at `legacy` to `target` when only `legacy` holds it, and says what it found.
///
/// `legacy` and `target` name the data itself (a settings file, a profiles folder), never the
/// tool's folder. `legacy` is `None` when the data has no old default place to look in.
///
/// - Only `target` present: [`Adoption::AlreadyThere`]. Neither: [`Adoption::Nothing`].
/// - Only `legacy` present: `target`'s folder is created owner-only, then a [`Shape::File`]
///   is hard-linked at `target` and its old name removed, and a [`Shape::Folder`] is renamed.
///   A hard link never replaces an existing `target`, and nothing is ever copied, so data on
///   another filesystem stays where it is. A `legacy` that is a symbolic link is not moved.
/// - Both present: when either is a symbolic link, the same file behind both is
///   [`Adoption::AlreadyThere`] and nothing is removed. So is one directory entry reached by
///   both paths (the same path given twice, or a link or junction above one of them). With no
///   link, one file under two names (a move cut short between its two steps) has its old name
///   removed. Anything else, including a failed identity check, is
///   [`Adoption::BothPresent`].
/// - After a failed step both paths are looked at again and decided by the rules above, so a
///   rival that moved the data first gives [`Adoption::AlreadyThere`]; with `target` still
///   absent the data stays at `legacy` ([`Adoption::KeptLegacy`]). A failed removal of the old
///   name keeps the old name in use, since the data may still be open under it.
///
/// Logs nothing: it can run before logging starts. Pass the result's
/// [`report`](Adoption::report) once logging is up.
#[must_use]
pub fn adopt(legacy: Option<&Path>, target: &Path, shape: Shape) -> Adoption {
    let mover = |from: &Path, to: &Path| match shape {
        Shape::File => fs::hard_link(from, to),
        Shape::Folder => fs::rename(from, to),
    };
    let steps = Steps {
        mover: &mover,
        remover: &|path: &Path| fs::remove_file(path),
        same_file: &|one: &Path, other: &Path| same_file::is_same_file(one, other),
        same_entry: &same_entry,
    };
    adopt_with(legacy, target, shape, &steps)
}

/// Whether two paths reach one directory entry: the same name in the same folder once every
/// link and junction on the way is resolved. Two hard links to one file are two entries.
fn same_entry(one: &Path, other: &Path) -> io::Result<bool> {
    Ok(fs::canonicalize(one)? == fs::canonicalize(other)?)
}

/// The filesystem steps [`adopt`] takes, apart so tests can make each one fail.
struct Steps<'a> {
    /// Puts the data at the new path: a hard link for a file, a rename for a folder.
    mover: &'a dyn Fn(&Path, &Path) -> io::Result<()>,
    /// Removes a file's old name.
    remover: &'a dyn Fn(&Path) -> io::Result<()>,
    /// Whether two paths name the same file.
    same_file: &'a dyn Fn(&Path, &Path) -> io::Result<bool>,
    /// Whether two paths reach the same directory entry, so removing one removes the other.
    same_entry: &'a dyn Fn(&Path, &Path) -> io::Result<bool>,
}

/// What is at a path, from its `symlink_metadata`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Presence {
    /// Nothing there: not found, or a file where a folder on the way should be (Linux and macOS
    /// report that as "not a directory", Windows as "not found"; either way the path cannot
    /// exist).
    Absent,
    /// A symbolic link, followed or not.
    Link,
    /// Anything else. A path whose metadata cannot be read for any other reason counts as
    /// present, so it is never treated as free to move onto or as empty.
    Entry,
}

fn presence(path: &Path) -> Presence {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Presence::Link,
        Ok(_) => Presence::Entry,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            Presence::Absent
        }
        Err(_) => Presence::Entry,
    }
}

fn adopt_with(legacy: Option<&Path>, target: &Path, shape: Shape, steps: &Steps<'_>) -> Adoption {
    let Some(legacy) = legacy else {
        return match presence(target) {
            Presence::Absent => Adoption::Nothing,
            Presence::Link | Presence::Entry => Adoption::AlreadyThere,
        };
    };
    match (presence(legacy), presence(target)) {
        (Presence::Absent, Presence::Absent) => Adoption::Nothing,
        (Presence::Absent, Presence::Link | Presence::Entry) => Adoption::AlreadyThere,
        (Presence::Link, Presence::Absent) => Adoption::KeptLegacy {
            legacy: legacy.to_path_buf(),
            reason: KeptReason::SymbolicLink,
        },
        (Presence::Entry, Presence::Absent) => move_data(legacy, target, shape, steps),
        (
            legacy_presence @ (Presence::Link | Presence::Entry),
            target_presence @ (Presence::Link | Presence::Entry),
        ) => both_present(
            legacy,
            target,
            legacy_presence == Presence::Link || target_presence == Presence::Link,
            steps,
        ),
    }
}

fn both_present(legacy: &Path, target: &Path, either_link: bool, steps: &Steps<'_>) -> Adoption {
    match ((steps.same_file)(legacy, target), either_link) {
        (Ok(true), true) => Adoption::AlreadyThere,
        (Ok(true), false) => one_file_found_twice(legacy, target, steps),
        (Ok(false) | Err(_), _) => Adoption::BothPresent {
            legacy: legacy.to_path_buf(),
        },
    }
}

/// Both paths name one file and neither is a link. One directory entry reached twice holds
/// the only copy, so nothing is removed; two entries are a move cut short between its link and
/// its remove, so the old name goes. When it cannot be told which, nothing is removed.
fn one_file_found_twice(legacy: &Path, target: &Path, steps: &Steps<'_>) -> Adoption {
    match (steps.same_entry)(legacy, target) {
        Ok(true) => Adoption::AlreadyThere,
        Ok(false) => match (steps.remover)(legacy) {
            Ok(()) => Adoption::Moved {
                from: legacy.to_path_buf(),
            },
            Err(error) => Adoption::KeptLegacy {
                legacy: legacy.to_path_buf(),
                reason: KeptReason::Failed(error),
            },
        },
        Err(_) => Adoption::BothPresent {
            legacy: legacy.to_path_buf(),
        },
    }
}

fn move_data(legacy: &Path, target: &Path, shape: Shape, steps: &Steps<'_>) -> Adoption {
    if let Some(parent) = target.parent()
        && let Err(error) = create_dir_all_owner_only(parent)
    {
        return Adoption::KeptLegacy {
            legacy: legacy.to_path_buf(),
            reason: KeptReason::Failed(error),
        };
    }
    let moved = (steps.mover)(legacy, target).and_then(|()| match shape {
        Shape::File => (steps.remover)(legacy),
        Shape::Folder => Ok(()),
    });
    match moved {
        Ok(()) => Adoption::Moved {
            from: legacy.to_path_buf(),
        },
        Err(error) => after_failure(legacy, target, error, steps),
    }
}

/// Decides from a fresh look at both paths after a failed move step, never from the error:
/// which step failed and why does not say where the data is now.
fn after_failure(legacy: &Path, target: &Path, error: io::Error, steps: &Steps<'_>) -> Adoption {
    match (presence(legacy), presence(target)) {
        (Presence::Absent, Presence::Absent) => Adoption::Nothing,
        (Presence::Absent, Presence::Link | Presence::Entry) => Adoption::AlreadyThere,
        (Presence::Link | Presence::Entry, Presence::Absent) => Adoption::KeptLegacy {
            legacy: legacy.to_path_buf(),
            reason: KeptReason::Failed(error),
        },
        (
            legacy_presence @ (Presence::Link | Presence::Entry),
            target_presence @ (Presence::Link | Presence::Entry),
        ) => both_present(
            legacy,
            target,
            legacy_presence == Presence::Link || target_presence == Presence::Link,
            steps,
        ),
    }
}

#[cfg(test)]
mod tests;
