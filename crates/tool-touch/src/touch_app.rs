use crate::models::TouchArgs;
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

    // Determine if we should update both times (default behavior when no -a or -m flags)
    // This mimics the behavior of running the touch command without any flags.
    let update_both = !args.access && !args.modify;

    let current_times =
        process_current_update_times(args, &file_obj, times, update_both).context(format!(
            "Failed to get current times for file: [{}]",
            &file_obj.display()
        ))?;

    let (final_atime, final_mtime) =
        get_times_to_use_when_updating_file(args, times, update_both, current_times);

    update_file_times(args, &file_obj, final_atime, final_mtime)?;

    Ok(())
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
/// - `update_both`: Whether both timestamps will be updated
///
/// # Returns
/// - `Ok(Some((atime, mtime)))`: Current file timestamps
/// - `Ok(None)`: Current times not needed
/// - `Err`: Metadata access failed
fn process_current_update_times(
    args: &TouchArgs,
    file_obj: &PathBuf,
    times: Option<(FileTime, FileTime)>,
    update_both: bool,
) -> Result<Option<(FileTime, FileTime)>> {
    let current_times = if times.is_none() || (!update_both && (!args.access || !args.modify)) {
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
/// - `update_both`: Whether to update both timestamps
/// - `current_times`: Current file timestamps for partial updates
///
/// # Returns
/// Tuple of (access_time, modification_time) to apply to file
fn get_times_to_use_when_updating_file(
    args: &TouchArgs,
    times: Option<(FileTime, FileTime)>,
    update_both: bool,
    current_times: Option<(FileTime, FileTime)>,
) -> (FileTime, FileTime) {
    let (final_atime, final_mtime) = if let Some((new_atime, new_mtime)) = times {
        let atime = if args.access || update_both {
            new_atime
        } else {
            current_times.unwrap().0
        };
        let mtime = if args.modify || update_both {
            new_mtime
        } else {
            current_times.unwrap().1
        };
        (atime, mtime)
    } else {
        // Use the current time
        let now = FileTime::now();
        let current = current_times.unwrap();
        let atime = if args.access || update_both {
            now
        } else {
            current.0
        };
        let mtime = if args.modify || update_both {
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
