use crate::models::{TouchArgs, TouchTimeWord};
use anyhow::{Context, Result};
use filetime::{set_file_times, set_symlink_file_times, FileTime};
use shared::system::get_full_filepath_from_string::get_full_filepath_from_string;
use std::fs::File;
use std::path::PathBuf;

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

    let path = get_full_filepath_from_string(&file.to_string());

    if path.exists() {
        Ok(true)
    } else if no_create {
        Ok(false)
    } else {
        // Create a file with appropriate permissions (0666 minus umask)
        File::create(path).context(format!("Failed to create file: {}", &file))?;
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
    let file_obj = get_full_filepath_from_string(&file.to_string());

    let file_exists = create_file_if_needed(file, args.no_create)?;

    if !file_exists && args.no_create {
        // File doesn't exist, and we shouldn't create it, so it is ok.
        return Ok(());
    }

    let times = args.get_current_filetime();

    // Determine which timestamps to update based on the time field from TouchArgs
    // This properly respects the --time option behavior
    let (update_access, update_modify) = determine_timestamps_to_update(args);

    let current_times =
        process_current_update_times(args, &file_obj, times, update_access, update_modify).context(format!(
            "Failed to get current times for file: [{}]",
            &file_obj.display()
        ))?;

    let (final_atime, final_mtime) =
        get_times_to_use_when_updating_file(args, times, update_access, update_modify, current_times);

    update_file_times(args, &file_obj, final_atime, final_mtime)?;

    Ok(())
}

/// Determines which timestamps should be updated based on TouchArgs configuration.
///
/// Respects the Unix touch precedence: individual flags (-a, -m) determine behavior,
/// with the TouchTimeWord enum representing the resolved decision from CLI parsing.
/// The cli_utils.rs already processes the precedence correctly.
///
/// # Parameters
/// - `args`: Touch arguments containing time specification and flags
///
/// # Returns
/// Tuple of (update_access, update_modify) booleans
fn determine_timestamps_to_update(args: &TouchArgs) -> (bool, bool) {
    // The TouchTimeWord enum already represents the correctly resolved decision
    // from cli_utils.rs which properly handles -a, -m, and --time precedence
    match args.time {
        TouchTimeWord::AccessOnly => (true, false),
        TouchTimeWord::ModifyOnly => (false, true),
        TouchTimeWord::AccessAndModify => (true, true),
    }
}

/// Retrieves current file timestamps when needed for partial updates.
///
/// Fetches access and modification times from file metadata when either:
/// - No new times are provided, or
/// - Partial update (only access or modify, not both)
///
/// # Parameters
/// - `args`: Touch arguments containing dereference options
/// - `file_obj`: Path to the target file
/// - `times`: Optional new timestamps
/// - `update_access`: Whether to update access time
/// - `update_modify`: Whether to update modification time
///
/// # Returns
/// - `Ok(Some((atime, mtime)))`: Current file timestamps
/// - `Ok(None)`: Current times not needed
/// - `Err`: Metadata access failed
fn process_current_update_times(
    args: &TouchArgs,
    file_obj: &PathBuf,
    times: Option<(FileTime, FileTime)>,
    update_access: bool,
    update_modify: bool,
) -> Result<Option<(FileTime, FileTime)>> {
    let current_times = if times.is_none() || (!update_access || !update_modify) {
        let metadata = if args.no_dereference {
            std::fs::symlink_metadata(&file_obj)?
        } else {
            std::fs::metadata(&file_obj)?
        };
        Some((
            FileTime::from_last_access_time(&metadata),
            FileTime::from_last_modification_time(&metadata),
        ))
    } else {
        None
    };
    Ok(current_times)
}

/// Determines final access and modification times for file update.
///
/// Calculates which timestamps to set based on user options, provided times,
/// and current file times. Uses current system time when no specific time provided.
///
/// # Parameters
/// - `args`: Touch arguments specifying which times to update
/// - `times`: Optional new timestamps tuple (access, modify)
/// - `update_access`: Whether to update access time
/// - `update_modify`: Whether to update modification time
/// - `current_times`: Current file timestamps for partial updates
///
/// # Returns
/// Tuple of (access_time, modification_time) to apply to file
fn get_times_to_use_when_updating_file(
    _args: &TouchArgs,
    times: Option<(FileTime, FileTime)>,
    update_access: bool,
    update_modify: bool,
    current_times: Option<(FileTime, FileTime)>,
) -> (FileTime, FileTime) {
    let (final_atime, final_mtime) = if let Some((new_atime, new_mtime)) = times {
        let atime = if update_access {
            new_atime
        } else {
            current_times.unwrap().0
        };
        let mtime = if update_modify {
            new_mtime
        } else {
            current_times.unwrap().1
        };
        (atime, mtime)
    } else {
        // Use the current time
        let now = FileTime::now();
        let current = current_times.unwrap();
        let atime = if update_access {
            now
        } else {
            current.0
        };
        let mtime = if update_modify {
            now
        } else {
            current.1
        };
        (atime, mtime)
    };
    (final_atime, final_mtime)
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
fn update_file_times(
    args: &TouchArgs,
    file_obj: &PathBuf,
    final_atime: FileTime,
    final_mtime: FileTime,
) -> Result<()> {
    if args.no_dereference {
        set_symlink_file_times(&file_obj, final_atime, final_mtime).context(format!(
            "Failed to set file times for symlink: [{}]",
            &file_obj.display()
        ))?;
    } else {
        set_file_times(&file_obj, final_atime, final_mtime).context(format!(
            "Failed to set file times for file: [{}]",
            &file_obj.display()
        ))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{TouchArgs, TouchTimeWord};
    use filetime::FileTime;
    use std::fs;
    use tempfile::TempDir;

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
    fn test_get_times_to_use_when_updating_file_with_new_times_access_only() {
        let args = create_test_touch_args();
        let new_time = FileTime::from_unix_time(1000000000, 0);
        let current_time = FileTime::from_unix_time(500000000, 0);
        let times = Some((new_time, new_time));
        let current_times = Some((current_time, current_time));

        let (final_atime, final_mtime) = get_times_to_use_when_updating_file(
            &args, times, true, false, current_times
        );

        assert_eq!(final_atime, new_time);
        assert_eq!(final_mtime, current_time);
    }

    #[test]
    fn test_get_times_to_use_when_updating_file_with_new_times_modify_only() {
        let args = create_test_touch_args();
        let new_time = FileTime::from_unix_time(1000000000, 0);
        let current_time = FileTime::from_unix_time(500000000, 0);
        let times = Some((new_time, new_time));
        let current_times = Some((current_time, current_time));

        let (final_atime, final_mtime) = get_times_to_use_when_updating_file(
            &args, times, false, true, current_times
        );

        assert_eq!(final_atime, current_time);
        assert_eq!(final_mtime, new_time);
    }

    #[test]
    fn test_get_times_to_use_when_updating_file_with_new_times_both() {
        let args = create_test_touch_args();
        let new_time = FileTime::from_unix_time(1000000000, 0);
        let current_time = FileTime::from_unix_time(500000000, 0);
        let times = Some((new_time, new_time));
        let current_times = Some((current_time, current_time));

        let (final_atime, final_mtime) = get_times_to_use_when_updating_file(
            &args, times, true, true, current_times
        );

        assert_eq!(final_atime, new_time);
        assert_eq!(final_mtime, new_time);
    }

    #[test]
    fn test_get_times_to_use_when_updating_file_no_new_times_access_only() {
        let args = create_test_touch_args();
        let current_time = FileTime::from_unix_time(500000000, 0);
        let current_times = Some((current_time, current_time));

        let (final_atime, final_mtime) = get_times_to_use_when_updating_file(
            &args, None, true, false, current_times
        );

        // Access time should be updated to "now" (we can't test exact time, but it should be recent)
        // Modify time should remain the current time
        assert_ne!(final_atime, current_time); // Should be updated
        assert_eq!(final_mtime, current_time); // Should remain current
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

        // Get original times
        let metadata = fs::metadata(&file_path).unwrap();
        let original_atime = FileTime::from_last_access_time(&metadata);
        let original_mtime = FileTime::from_last_modification_time(&metadata);

        // Wait a bit to ensure time difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        let mut args = create_test_touch_args();
        args.time = TouchTimeWord::AccessOnly;
        args.files = vec![file_path.to_str().unwrap().to_string()];

        let result = touch_file(file_path.to_str().unwrap(), &args);
        assert!(result.is_ok());

        // Check that only access time was updated
        let metadata = fs::metadata(&file_path).unwrap();
        let new_atime = FileTime::from_last_access_time(&metadata);
        let new_mtime = FileTime::from_last_modification_time(&metadata);

        assert_ne!(new_atime, original_atime); // Access time should be different
        assert_eq!(new_mtime, original_mtime); // Modification time should be same
    }

    #[test]
    fn test_touch_file_integration_modify_only() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_modify.txt");
        fs::write(&file_path, "test").unwrap();

        // Get original times
        let metadata = fs::metadata(&file_path).unwrap();
        let original_atime = FileTime::from_last_access_time(&metadata);
        let original_mtime = FileTime::from_last_modification_time(&metadata);

        // Wait a bit to ensure time difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        let mut args = create_test_touch_args();
        args.time = TouchTimeWord::ModifyOnly;
        args.files = vec![file_path.to_str().unwrap().to_string()];

        let result = touch_file(file_path.to_str().unwrap(), &args);
        assert!(result.is_ok());

        // Check that only modification time was updated
        let metadata = fs::metadata(&file_path).unwrap();
        let new_atime = FileTime::from_last_access_time(&metadata);
        let new_mtime = FileTime::from_last_modification_time(&metadata);

        assert_eq!(new_atime, original_atime); // Access time should be same
        assert_ne!(new_mtime, original_mtime); // Modification time should be different
    }

    #[test]
    fn test_touch_file_integration_both_times() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_both.txt");
        fs::write(&file_path, "test").unwrap();

        // Get original times
        let metadata = fs::metadata(&file_path).unwrap();
        let original_atime = FileTime::from_last_access_time(&metadata);
        let original_mtime = FileTime::from_last_modification_time(&metadata);

        // Wait a bit to ensure time difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        let mut args = create_test_touch_args();
        args.time = TouchTimeWord::AccessAndModify;
        args.files = vec![file_path.to_str().unwrap().to_string()];

        let result = touch_file(file_path.to_str().unwrap(), &args);
        assert!(result.is_ok());

        // Check that both times were updated
        let metadata = fs::metadata(&file_path).unwrap();
        let new_atime = FileTime::from_last_access_time(&metadata);
        let new_mtime = FileTime::from_last_modification_time(&metadata);

        assert_ne!(new_atime, original_atime); // Access time should be different
        assert_ne!(new_mtime, original_mtime); // Modification time should be different
    }
}
