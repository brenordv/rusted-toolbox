//! Owner-only directory creation and file opening. On Unix the entries are
//! restricted to the owner (0o700 directories, 0o600 files); on Windows, which
//! has no mode bits, both fall back to default ACL inheritance from the parent.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

/// Creates `path` and any missing parents. On Unix every directory it creates
/// is owner-only (0o700); a directory that already exists keeps its
/// permissions.
#[cfg(unix)]
pub fn create_dir_all_owner_only(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

/// Creates `path` and any missing parents. Windows has no mode bits; the
/// directories inherit the parent's ACL.
#[cfg(not(unix))]
pub fn create_dir_all_owner_only(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Opens `path` with the given options, forcing owner-read/write (0o600). A
/// file that already exists is tightened to the same mode through the open
/// handle (fchmod semantics: no path re-resolution). When the tighten fails,
/// the error is returned instead of the handle: handing out a file whose
/// permissions could not be restricted is the outcome this function exists to
/// prevent.
#[cfg(unix)]
pub fn open_owner_only(options: &mut OpenOptions, path: &Path) -> io::Result<File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let file = options.mode(0o600).open(path)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;

    Ok(file)
}

/// Opens `path` with the given options. Windows has no mode bits; the file
/// inherits the parent directory's ACL.
#[cfg(not(unix))]
pub fn open_owner_only(options: &mut OpenOptions, path: &Path) -> io::Result<File> {
    options.open(path)
}

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn created_directories_have_no_group_or_other_bits() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("nested").join("inner");

        create_dir_all_owner_only(&target).unwrap();

        // mkdir(2) applies `mode & !umask`, so this asserts only that
        // group/other bits are clear; asserting owner bits would fail under an
        // owner-bit-clearing umask.
        for dir in [root.path().join("nested"), target] {
            let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
            assert_eq!(
                mode & 0o077,
                0,
                "created dir must have no group/other bits: {}",
                dir.display()
            );
        }
    }

    // The file assertions below are exact: open_owner_only ends in a
    // handle-based set_permissions (fchmod), which umask does not filter.

    #[test]
    fn created_file_is_owner_only() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("owned.txt");

        open_owner_only(OpenOptions::new().create(true).append(true), &path).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "created file must be owner-only");
    }

    #[test]
    fn pre_existing_wide_file_is_tightened_on_open() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("wide.txt");
        std::fs::write(&path, b"old content").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        open_owner_only(OpenOptions::new().write(true), &path).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "pre-existing file must be tightened on open"
        );
    }
}
