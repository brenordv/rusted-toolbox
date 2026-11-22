use crate::models::{TouchArgs, TouchTimeWord};
use anyhow::{Context, Result, anyhow};
use common_utils::file_system::get_full_filepath_from_string;
use filetime::FileTime;
use std::fs::{self, File, FileTimes};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Creates a file if it doesn't exist based on the no_create flag.
///
/// # Parameters
/// - `file`: File path or "-" for stdout
/// - `no_create`: Skip file creation if true
///
/// # Returns
/// - `Ok(true)`: File exists or was created
/// - `Ok(false)`: File doesn't exist and no_create is true
/// - `Err`: File creation failed
fn create_file_if_needed(file: &str, no_create: bool) -> Result<bool> {
    if file == "-" {
        return Ok(true); // stdout always exists
    }

    let path = get_full_filepath_from_string(file);

    if path.exists() {
        Ok(true)
    } else if no_create {
        Ok(false)
    } else {
        // Create a file with appropriate permissions (0666 minus umask)
        File::create(path).context(format!("Failed to create file: {}", file))?;
        Ok(true)
    }
}

/// Updates file timestamps, mimicking Unix `touch` command behavior.
///
/// Creates the file if it doesn't exist (unless no_create is set), then updates
/// access and/or modification times based on the provided arguments.
///
/// # Parameters
/// - `file`: File path or "-" for stdout (no-op)
/// - `args`: Touch arguments containing timestamp and behavior options
///
/// # Returns
/// - `Ok(())` on successful completion
/// - `Err`: File creation, timestamp retrieval, or update failures
///
/// # Behavior
/// - Stdout ("-") is treated as no-op
/// - Updates both timestamps by default unless -a or -m specified
/// - Uses current time or user-specified time/reference file
pub fn touch_file(file: &str, args: &TouchArgs) -> Result<()> {
    // Handle stdout specially
    if file == "-" {
        // On most systems, touching stdout is a no-op
        // We just return success without error like the real touch command
        return Ok(());
    }

    // Create a file if needed
    let file_obj = get_full_filepath_from_string(file);

    let file_exists = create_file_if_needed(file, args.no_create)?;

    if !file_exists && args.no_create {
        // File doesn't exist, and we shouldn't create it, so it is ok.
        return Ok(());
    }

    let times = args.get_current_filetime();

    // Determine which timestamps to update based on the time field from TouchArgs
    // This properly respects the --time option behavior
    let (update_access, update_modify) = determine_timestamps_to_update(args);

    let (final_atime, final_mtime) =
        resolve_final_times(args, &file_obj, times, update_access, update_modify).with_context(
            || format!("Failed to resolve times for file: [{}]", file_obj.display()),
        )?;

    update_file_times(args, &file_obj, final_atime, final_mtime)?;

    Ok(())
}

/// Determines which timestamps should be updated based on TouchArgs configuration.
///
/// Respects the Unix touch precedence: individual flags (-a, -m) determine behavior,
/// with the TouchTimeWord enum representing the resolved decision from CLI parsing.
/// The tools already processes the precedence correctly.
///
/// # Parameters
/// - `args`: Touch arguments containing time specification and flags
///
/// # Returns
/// Tuple of (update_access, update_modify) booleans
fn determine_timestamps_to_update(args: &TouchArgs) -> (bool, bool) {
    // The TouchTimeWord enum already represents the correctly resolved decision
    // from tools which properly handles -a, -m, and --time precedence
    match args.time {
        TouchTimeWord::AccessOnly => (true, false),
        TouchTimeWord::ModifyOnly => (false, true),
        TouchTimeWord::AccessAndModify => (true, true),
    }
}

/// Reads the file's current access and modification times, honoring the
/// no-dereference option (symlink metadata vs target metadata).
///
/// # Errors
/// Returns the metadata access failure.
fn read_current_times(args: &TouchArgs, file_obj: &Path) -> Result<(FileTime, FileTime)> {
    let metadata = if args.no_dereference {
        std::fs::symlink_metadata(file_obj)?
    } else {
        std::fs::metadata(file_obj)?
    };
    Ok((
        FileTime::from(metadata.accessed()?),
        FileTime::from(metadata.modified()?),
    ))
}

/// Combines the requested pair with the file's current pair: each timestamp
/// takes the new value only when its update flag is set.
fn combine_times(
    new_times: (FileTime, FileTime),
    current_times: (FileTime, FileTime),
    update_access: bool,
    update_modify: bool,
) -> (FileTime, FileTime) {
    (
        if update_access {
            new_times.0
        } else {
            current_times.0
        },
        if update_modify {
            new_times.1
        } else {
            current_times.1
        },
    )
}

/// Resolves the final access/modification pair to set: the explicit source
/// pair when one was given, the current time for both otherwise. The file's
/// current times are read only for a partial update, where the untouched
/// timestamp must keep its value.
///
/// # Errors
/// Returns the metadata access failure from a partial update's current-times
/// read.
fn resolve_final_times(
    args: &TouchArgs,
    file_obj: &Path,
    times: Option<(FileTime, FileTime)>,
    update_access: bool,
    update_modify: bool,
) -> Result<(FileTime, FileTime)> {
    let new_times = times.unwrap_or_else(|| {
        let now = FileTime::now();
        (now, now)
    });

    if update_access && update_modify {
        return Ok(new_times);
    }

    let current_times = read_current_times(args, file_obj)?;
    Ok(combine_times(
        new_times,
        current_times,
        update_access,
        update_modify,
    ))
}

/// Applies new timestamps to file or symlink.
///
/// Updates file times using the appropriate system call based on
/// a no_dereference option (symlink vs target file).
///
/// # Parameters
/// - `args`: Touch arguments containing dereference options
/// - `file_obj`: Path to target file/symlink
/// - `final_atime`: New access time
/// - `final_mtime`: New modification time
///
/// # Errors
/// Returns an error when either time is out of range for the platform, or
/// when the timestamp update fails.
fn update_file_times(
    args: &TouchArgs,
    file_obj: &PathBuf,
    final_atime: FileTime,
    final_mtime: FileTime,
) -> Result<()> {
    let times = FileTimes::new()
        .set_accessed(to_system_time(final_atime)?)
        .set_modified(to_system_time(final_mtime)?);

    if args.no_dereference {
        fs::set_times_nofollow(file_obj, times).context(format!(
            "Failed to set file times for symlink: [{}]",
            file_obj.display()
        ))?;
    } else {
        fs::set_times(file_obj, times).context(format!(
            "Failed to set file times for file: [{}]",
            file_obj.display()
        ))?;
    }

    Ok(())
}

/// Converts a [`FileTime`] into the [`SystemTime`] the std timestamp calls take.
///
/// # Errors
/// Returns an error when the platform cannot represent the time (on Windows,
/// anything before 1601-01-01T00:00:00Z).
fn to_system_time(time: FileTime) -> Result<SystemTime> {
    let seconds = time.unix_seconds();
    let whole_seconds = Duration::from_secs(seconds.unsigned_abs());
    let whole = if seconds < 0 {
        UNIX_EPOCH.checked_sub(whole_seconds)
    } else {
        UNIX_EPOCH.checked_add(whole_seconds)
    };

    whole
        .and_then(|t| t.checked_add(Duration::from_nanos(u64::from(time.nanoseconds()))))
        .ok_or_else(|| {
            anyhow!(
                "Time out of range for this platform: {} seconds from the Unix epoch",
                seconds
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{TouchArgs, TouchTimeWord};
    use filetime::FileTime;
    use std::fs;
    use tempfile::TempDir;

    /// Both timestamps set to 2000-01-01 00:00:00 UTC, the fixed past instant
    /// the tests pin files and links to.
    fn pinned_file_times() -> FileTimes {
        let pinned = to_system_time(FileTime::from_unix_time(946_684_800, 0)).unwrap();
        FileTimes::new().set_accessed(pinned).set_modified(pinned)
    }

    /// Pins both timestamps of `path` to a fixed past instant and returns the
    /// values read back from the filesystem. Starting from a pinned past
    /// instant makes "updated" and "unchanged" assertions unambiguous
    /// regardless of when the file was created.
    fn pin_file_times(path: &std::path::Path) -> (FileTime, FileTime) {
        fs::set_times(path, pinned_file_times()).unwrap();
        let metadata = fs::metadata(path).unwrap();
        (
            FileTime::from(metadata.accessed().unwrap()),
            FileTime::from(metadata.modified().unwrap()),
        )
    }

    /// Helper function to create a TouchArgs with default values for testing
    fn create_test_touch_args() -> TouchArgs {
        TouchArgs {
            access: false,
            no_create: false,
            date: None,
            ignore: false,
            no_dereference: false,
            modify: false,
            reference: None,
            time_spec: None,
            time: TouchTimeWord::AccessAndModify,
            files: vec!["test.txt".to_string()],
        }
    }

    #[test]
    fn test_determine_timestamps_to_update_access_only() {
        let mut args = create_test_touch_args();
        args.time = TouchTimeWord::AccessOnly;

        let (update_access, update_modify) = determine_timestamps_to_update(&args);

        assert!(update_access);
        assert!(!update_modify);
    }

    #[test]
    fn test_determine_timestamps_to_update_modify_only() {
        let mut args = create_test_touch_args();
        args.time = TouchTimeWord::ModifyOnly;

        let (update_access, update_modify) = determine_timestamps_to_update(&args);

        assert!(!update_access);
        assert!(update_modify);
    }

    #[test]
    fn test_determine_timestamps_to_update_both() {
        let mut args = create_test_touch_args();
        args.time = TouchTimeWord::AccessAndModify;

        let (update_access, update_modify) = determine_timestamps_to_update(&args);

        assert!(update_access);
        assert!(update_modify);
    }

    #[test]
    fn combine_times_access_only_takes_new_access_and_keeps_modify() {
        let new_time = FileTime::from_unix_time(1000000000, 0);
        let current_time = FileTime::from_unix_time(500000000, 0);

        let (final_atime, final_mtime) = combine_times(
            (new_time, new_time),
            (current_time, current_time),
            true,
            false,
        );

        assert_eq!(final_atime, new_time);
        assert_eq!(final_mtime, current_time);
    }

    #[test]
    fn combine_times_modify_only_takes_new_modify_and_keeps_access() {
        let new_time = FileTime::from_unix_time(1000000000, 0);
        let current_time = FileTime::from_unix_time(500000000, 0);

        let (final_atime, final_mtime) = combine_times(
            (new_time, new_time),
            (current_time, current_time),
            false,
            true,
        );

        assert_eq!(final_atime, current_time);
        assert_eq!(final_mtime, new_time);
    }

    #[test]
    fn combine_times_both_flags_take_both_new_values() {
        let new_time = FileTime::from_unix_time(1000000000, 0);
        let current_time = FileTime::from_unix_time(500000000, 0);

        let (final_atime, final_mtime) = combine_times(
            (new_time, new_time),
            (current_time, current_time),
            true,
            true,
        );

        assert_eq!(final_atime, new_time);
        assert_eq!(final_mtime, new_time);
    }

    #[test]
    fn resolve_final_times_full_update_needs_no_existing_file() {
        // Both flags set with an explicit pair: the file's current times are
        // irrelevant, so resolution succeeds even for a path with no metadata.
        let args = create_test_touch_args();
        let new_time = FileTime::from_unix_time(1000000000, 0);

        let (final_atime, final_mtime) = resolve_final_times(
            &args,
            Path::new("does-not-exist.txt"),
            Some((new_time, new_time)),
            true,
            true,
        )
        .unwrap();

        assert_eq!(final_atime, new_time);
        assert_eq!(final_mtime, new_time);
    }

    #[test]
    fn resolve_final_times_partial_update_errors_without_the_file() {
        let args = create_test_touch_args();
        let new_time = FileTime::from_unix_time(1000000000, 0);

        let result = resolve_final_times(
            &args,
            Path::new("does-not-exist.txt"),
            Some((new_time, new_time)),
            true,
            false,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_create_file_if_needed_file_exists() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("existing_file.txt");
        fs::write(&file_path, "test content").unwrap();

        let result = create_file_if_needed(file_path.to_str().unwrap(), false);

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_create_file_if_needed_file_not_exists_create() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("new_file.txt");

        let result = create_file_if_needed(file_path.to_str().unwrap(), false);

        assert!(result.is_ok());
        assert!(result.unwrap());
        assert!(file_path.exists());
    }

    #[test]
    fn test_create_file_if_needed_file_not_exists_no_create() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nonexistent_file.txt");

        let result = create_file_if_needed(file_path.to_str().unwrap(), true);

        assert!(result.is_ok());
        assert!(!result.unwrap());
        assert!(!file_path.exists());
    }

    #[test]
    fn test_create_file_if_needed_stdout() {
        let result = create_file_if_needed("-", false);

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_touch_file_integration_access_only() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_access.txt");
        fs::write(&file_path, "test").unwrap();

        // Let any file-content scanner triggered by the write above finish, so
        // a scan does not bump the access time mid-test, then pin the
        // timestamps.
        std::thread::sleep(std::time::Duration::from_millis(100));
        let (original_atime, original_mtime) = pin_file_times(&file_path);

        let mut args = create_test_touch_args();
        args.time = TouchTimeWord::AccessOnly;
        args.files = vec![file_path.to_str().unwrap().to_string()];

        let result = touch_file(file_path.to_str().unwrap(), &args);
        assert!(result.is_ok());

        // Check that only access time was updated
        let metadata = fs::metadata(&file_path).unwrap();
        let new_atime = FileTime::from(metadata.accessed().unwrap());
        let new_mtime = FileTime::from(metadata.modified().unwrap());

        assert_ne!(new_atime, original_atime); // Access time should be different
        assert_eq!(new_mtime, original_mtime); // Modification time should be same
    }

    #[test]
    fn test_touch_file_integration_modify_only() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_modify.txt");
        fs::write(&file_path, "test").unwrap();

        // Let any file-content scanner triggered by the write above finish, so
        // a scan does not bump the access time mid-test, then pin the
        // timestamps.
        std::thread::sleep(std::time::Duration::from_millis(100));
        let (original_atime, original_mtime) = pin_file_times(&file_path);

        let mut args = create_test_touch_args();
        args.time = TouchTimeWord::ModifyOnly;
        args.files = vec![file_path.to_str().unwrap().to_string()];

        let result = touch_file(file_path.to_str().unwrap(), &args);
        assert!(result.is_ok());

        // Check that only modification time was updated
        let metadata = fs::metadata(&file_path).unwrap();
        let new_atime = FileTime::from(metadata.accessed().unwrap());
        let new_mtime = FileTime::from(metadata.modified().unwrap());

        assert_eq!(new_atime, original_atime); // Access time should be same
        assert_ne!(new_mtime, original_mtime); // Modification time should be different
    }

    #[test]
    fn test_touch_file_integration_both_times() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_both.txt");
        fs::write(&file_path, "test").unwrap();

        // Let any file-content scanner triggered by the write above finish, so
        // a scan does not bump the access time mid-test, then pin the
        // timestamps.
        std::thread::sleep(std::time::Duration::from_millis(100));
        let (original_atime, original_mtime) = pin_file_times(&file_path);

        let mut args = create_test_touch_args();
        args.time = TouchTimeWord::AccessAndModify;
        args.files = vec![file_path.to_str().unwrap().to_string()];

        let result = touch_file(file_path.to_str().unwrap(), &args);
        assert!(result.is_ok());

        // Check that both times were updated
        let metadata = fs::metadata(&file_path).unwrap();
        let new_atime = FileTime::from(metadata.accessed().unwrap());
        let new_mtime = FileTime::from(metadata.modified().unwrap());

        assert_ne!(new_atime, original_atime); // Access time should be different
        assert_ne!(new_mtime, original_mtime); // Modification time should be different
    }

    #[test]
    fn to_system_time_round_trips_times_on_both_sides_of_the_epoch() {
        // Nanoseconds in whole 100 ns steps: Windows keeps file times at that resolution.
        for time in [
            FileTime::from_unix_time(946_684_800, 123_456_700),
            FileTime::from_unix_time(-1_000, 500_000_000),
        ] {
            let system_time = to_system_time(time).unwrap();

            assert_eq!(FileTime::from(system_time), time);
        }
    }

    #[cfg(windows)]
    #[test]
    fn update_file_times_rejects_a_time_before_1601_and_leaves_the_file_alone() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("too_old.txt");
        fs::write(&file_path, "test").unwrap();
        let (original_atime, original_mtime) = pin_file_times(&file_path);
        // One day before 1601-01-01T00:00:00Z, where Windows file time starts.
        let too_old = FileTime::from_unix_time(-11_644_473_600 - 86_400, 0);
        let args = create_test_touch_args();

        let result = update_file_times(&args, &file_path, too_old, too_old);

        let error = result.unwrap_err();
        assert!(error.to_string().contains("out of range"), "{error:#}");
        let metadata = fs::metadata(&file_path).unwrap();
        assert_eq!(FileTime::from(metadata.accessed().unwrap()), original_atime);
        assert_eq!(FileTime::from(metadata.modified().unwrap()), original_mtime);
    }

    #[test]
    fn touch_file_updates_a_directorys_times() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("a_directory");
        fs::create_dir(&dir_path).unwrap();
        let (original_atime, original_mtime) = pin_file_times(&dir_path);
        let mut args = create_test_touch_args();
        args.files = vec![dir_path.to_str().unwrap().to_string()];

        touch_file(dir_path.to_str().unwrap(), &args).unwrap();

        let metadata = fs::metadata(&dir_path).unwrap();
        assert_ne!(FileTime::from(metadata.accessed().unwrap()), original_atime);
        assert_ne!(FileTime::from(metadata.modified().unwrap()), original_mtime);
    }

    #[test]
    fn touch_file_updates_a_read_only_files_times() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("read_only.txt");
        fs::write(&file_path, "test").unwrap();

        // Let any file-content scanner triggered by the write above finish,
        // then make the file read-only and pin its timestamps.
        std::thread::sleep(std::time::Duration::from_millis(100));
        let mut permissions = fs::metadata(&file_path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&file_path, permissions).unwrap();
        let (original_atime, original_mtime) = pin_file_times(&file_path);
        let mut args = create_test_touch_args();
        args.files = vec![file_path.to_str().unwrap().to_string()];

        touch_file(file_path.to_str().unwrap(), &args).unwrap();

        let metadata = fs::metadata(&file_path).unwrap();
        assert_ne!(FileTime::from(metadata.accessed().unwrap()), original_atime);
        assert_ne!(FileTime::from(metadata.modified().unwrap()), original_mtime);
        temp_dir.close().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn touch_file_no_dereference_moves_the_link_and_leaves_the_target() {
        let temp_dir = TempDir::new().unwrap();
        let target_path = temp_dir.path().join("target.txt");
        let link_path = temp_dir.path().join("link.txt");
        fs::write(&target_path, "test").unwrap();
        std::os::unix::fs::symlink(&target_path, &link_path).unwrap();
        fs::set_times_nofollow(&link_path, pinned_file_times()).unwrap();
        let pinned_link_mtime = FileTime::from(
            fs::symlink_metadata(&link_path)
                .unwrap()
                .modified()
                .unwrap(),
        );
        let (_, pinned_target_mtime) = pin_file_times(&target_path);
        let mut args = create_test_touch_args();
        args.no_dereference = true;
        args.files = vec![link_path.to_str().unwrap().to_string()];

        touch_file(link_path.to_str().unwrap(), &args).unwrap();

        let link_mtime = FileTime::from(
            fs::symlink_metadata(&link_path)
                .unwrap()
                .modified()
                .unwrap(),
        );
        let target_mtime = FileTime::from(fs::metadata(&target_path).unwrap().modified().unwrap());
        assert_ne!(link_mtime, pinned_link_mtime);
        assert_eq!(target_mtime, pinned_target_mtime);
    }
}
