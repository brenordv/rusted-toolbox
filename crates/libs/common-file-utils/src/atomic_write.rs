//! Atomic file replacement: write into a temporary file in the destination's
//! directory, then rename it over the destination. A failed write never
//! touches the destination, and no temp file survives either outcome.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::warn;

/// Builds the temp-file builder used for the staged output. On Unix the mode
/// is widened to 0o666 before the umask applies, matching what a plain
/// `File::create` would produce; tempfile's default 0o600 would otherwise
/// survive the rename and tighten every output.
#[cfg(unix)]
fn output_temp_builder() -> tempfile::Builder<'static, 'static> {
    use std::os::unix::fs::PermissionsExt;
    let mut builder = tempfile::Builder::new();
    builder.permissions(std::fs::Permissions::from_mode(0o666));
    builder
}

/// Builds the temp-file builder used for the staged output. Windows derives
/// effective permissions from the directory's ACL, so the default builder is
/// already right.
#[cfg(not(unix))]
fn output_temp_builder() -> tempfile::Builder<'static, 'static> {
    tempfile::Builder::new()
}

/// Runs `write` against a temporary file in the destination's directory, then
/// renames it over `output_path`. A failed `write` leaves the destination
/// untouched and removes the temp file, which matters most when the
/// destination is also the input: a truncating write there would destroy the
/// only copy. The temp file lives next to the destination because the rename
/// must not cross filesystems. A destination being replaced keeps its own
/// permissions rather than inheriting the temp file's.
///
/// # Errors
/// Fails when the temp file cannot be created, when `write` fails (its error
/// is returned unchanged), or when the rename over `output_path` fails.
pub fn write_via_temp(
    output_path: &Path,
    write: impl FnOnce(&PathBuf) -> Result<()>,
) -> Result<()> {
    // A bare filename has an empty parent, which tempfile would reject.
    // Only root paths have no parent at all; those stage in the current
    // directory and fail at the rename.
    let output_dir = match output_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };

    let temp = output_temp_builder()
        .tempfile_in(output_dir)
        .with_context(|| {
            format!(
                "Cannot create a temporary file in '{}'",
                output_dir.display()
            )
        })?
        .into_temp_path();
    let temp_file = temp.to_path_buf();

    if let Err(write_error) = write(&temp_file) {
        if let Err(close_error) = temp.close() {
            warn!(
                path = %temp_file.display(),
                error = %close_error,
                "Failed to remove the temporary output file"
            );
        }
        return Err(write_error);
    }

    if let Ok(existing) = std::fs::metadata(output_path)
        && let Err(error) = std::fs::set_permissions(&temp_file, existing.permissions())
    {
        warn!(
            path = %output_path.display(),
            error = %error,
            "Failed to copy the destination's permissions to the new output"
        );
    }

    // The failed-rename arm removes the temp file itself: callers exit
    // through process::exit paths that skip Drop, so deletion-on-drop
    // cannot be relied on, and the staged file may hold plaintext.
    if let Err(persist_error) = temp.persist(output_path) {
        if let Err(close_error) = persist_error.path.close() {
            warn!(
                path = %temp_file.display(),
                error = %close_error,
                "Failed to remove the temporary output file"
            );
        }
        return Err(anyhow::Error::new(persist_error.error).context(format!(
            "Cannot move temporary file '{}' over '{}'",
            temp_file.display(),
            output_path.display()
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory_entry_count(dir: &Path) -> usize {
        std::fs::read_dir(dir).unwrap().count()
    }

    #[test]
    fn write_via_temp_failure_leaves_an_existing_destination_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.bin");
        std::fs::write(&dest, b"original").unwrap();

        let result = write_via_temp(&dest, |_| anyhow::bail!("writer failed"));

        assert!(result.is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), b"original");
        assert_eq!(
            directory_entry_count(dir.path()),
            1,
            "the failed write must not leave a temp file behind"
        );
    }

    #[test]
    fn write_via_temp_success_replaces_the_destination_without_leftovers() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.bin");
        std::fs::write(&dest, b"old").unwrap();

        write_via_temp(&dest, |temp_file| {
            std::fs::write(temp_file, b"new").map_err(Into::into)
        })
        .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), b"new");
        assert_eq!(directory_entry_count(dir.path()), 1);
    }

    #[test]
    fn write_via_temp_creates_a_missing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("fresh.bin");

        write_via_temp(&dest, |temp_file| {
            std::fs::write(temp_file, b"fresh").map_err(Into::into)
        })
        .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), b"fresh");
        assert_eq!(directory_entry_count(dir.path()), 1);
    }

    #[test]
    fn write_via_temp_rename_failure_removes_the_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("occupied");
        std::fs::create_dir(&dest).unwrap();

        let result = write_via_temp(&dest, |temp_file| {
            std::fs::write(temp_file, b"staged").map_err(Into::into)
        });

        assert!(result.is_err(), "renaming over a directory must fail");
        assert!(dest.is_dir());
        assert_eq!(
            directory_entry_count(dir.path()),
            1,
            "the failed rename must not leave a temp file behind"
        );
    }
}
