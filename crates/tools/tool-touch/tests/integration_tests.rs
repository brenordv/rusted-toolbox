use filetime::{FileTime, set_file_times};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Helper function to get the path to the touch binary.
/// Cargo sets this to the exact built binary for the current profile and OS.
fn get_touch_binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_touch")
}

/// Pins both timestamps of `path` to a fixed past instant and returns the
/// values read back from the filesystem. Starting from a pinned past instant
/// makes "updated" and "unchanged" assertions unambiguous regardless of when
/// the file was created.
fn pin_file_times(path: &std::path::Path) -> (FileTime, FileTime) {
    let pinned = FileTime::from_unix_time(946_684_800, 0); // 2000-01-01 00:00:00 UTC
    set_file_times(path, pinned, pinned).unwrap();
    let metadata = fs::metadata(path).unwrap();
    (
        FileTime::from_last_access_time(&metadata),
        FileTime::from_last_modification_time(&metadata),
    )
}

#[test]
fn test_cli_time_option_access_only() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test_cli_access.txt");
    fs::write(&file_path, "test content").unwrap();

    // Let any file-content scanner triggered by the write above finish, so a
    // scan does not bump the access time mid-test, then pin the timestamps.
    std::thread::sleep(std::time::Duration::from_millis(100));
    let (original_atime, original_mtime) = pin_file_times(&file_path);

    // Run touch with --time=access
    let output = Command::new(get_touch_binary_path())
        .arg("--time=access")
        .arg(file_path.to_str().unwrap())
        .output()
        .expect("Failed to execute touch command");

    assert!(
        output.status.success(),
        "Touch command failed: {:?}",
        output
    );

    // Check that only access time was updated
    let metadata = fs::metadata(&file_path).unwrap();
    let new_atime = FileTime::from_last_access_time(&metadata);
    let new_mtime = FileTime::from_last_modification_time(&metadata);

    assert_ne!(
        new_atime, original_atime,
        "Access time should have been updated"
    );
    assert_eq!(
        new_mtime, original_mtime,
        "Modification time should remain unchanged"
    );
}

#[test]
fn test_cli_time_option_modify_only() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test_cli_modify.txt");
    fs::write(&file_path, "test content").unwrap();

    // Let any file-content scanner triggered by the write above finish, so a
    // scan does not bump the access time mid-test, then pin the timestamps.
    std::thread::sleep(std::time::Duration::from_millis(100));
    let (original_atime, original_mtime) = pin_file_times(&file_path);

    // Run touch with --time=modify
    let output = Command::new(get_touch_binary_path())
        .arg("--time=modify")
        .arg(file_path.to_str().unwrap())
        .output()
        .expect("Failed to execute touch command");

    assert!(
        output.status.success(),
        "Touch command failed: {:?}",
        output
    );

    // Check that only modification time was updated
    let metadata = fs::metadata(&file_path).unwrap();
    let new_atime = FileTime::from_last_access_time(&metadata);
    let new_mtime = FileTime::from_last_modification_time(&metadata);

    assert_eq!(
        new_atime, original_atime,
        "Access time should remain unchanged"
    );
    assert_ne!(
        new_mtime, original_mtime,
        "Modification time should have been updated"
    );
}

#[test]
fn test_cli_time_option_atime_alias() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test_cli_atime.txt");
    fs::write(&file_path, "test content").unwrap();

    // Let any file-content scanner triggered by the write above finish, so a
    // scan does not bump the access time mid-test, then pin the timestamps.
    std::thread::sleep(std::time::Duration::from_millis(100));
    let (original_atime, original_mtime) = pin_file_times(&file_path);

    // Run touch with --time=atime (alias for access)
    let output = Command::new(get_touch_binary_path())
        .arg("--time=atime")
        .arg(file_path.to_str().unwrap())
        .output()
        .expect("Failed to execute touch command");

    assert!(
        output.status.success(),
        "Touch command failed: {:?}",
        output
    );

    // Check that only access time was updated
    let metadata = fs::metadata(&file_path).unwrap();
    let new_atime = FileTime::from_last_access_time(&metadata);
    let new_mtime = FileTime::from_last_modification_time(&metadata);

    assert_ne!(
        new_atime, original_atime,
        "Access time should have been updated"
    );
    assert_eq!(
        new_mtime, original_mtime,
        "Modification time should remain unchanged"
    );
}

#[test]
fn test_cli_time_option_mtime_alias() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test_cli_mtime.txt");
    fs::write(&file_path, "test content").unwrap();

    // Let any file-content scanner triggered by the write above finish, so a
    // scan does not bump the access time mid-test, then pin the timestamps.
    std::thread::sleep(std::time::Duration::from_millis(100));
    let (original_atime, original_mtime) = pin_file_times(&file_path);

    // Run touch with --time=mtime (alias for modify)
    let output = Command::new(get_touch_binary_path())
        .arg("--time=mtime")
        .arg(file_path.to_str().unwrap())
        .output()
        .expect("Failed to execute touch command");

    assert!(
        output.status.success(),
        "Touch command failed: {:?}",
        output
    );

    // Check that only modification time was updated
    let metadata = fs::metadata(&file_path).unwrap();
    let new_atime = FileTime::from_last_access_time(&metadata);
    let new_mtime = FileTime::from_last_modification_time(&metadata);

    assert_eq!(
        new_atime, original_atime,
        "Access time should remain unchanged"
    );
    assert_ne!(
        new_mtime, original_mtime,
        "Modification time should have been updated"
    );
}

#[test]
fn test_cli_time_option_use_alias() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test_cli_use.txt");
    fs::write(&file_path, "test content").unwrap();

    // Let any file-content scanner triggered by the write above finish, so a
    // scan does not bump the access time mid-test, then pin the timestamps.
    std::thread::sleep(std::time::Duration::from_millis(100));
    let (original_atime, original_mtime) = pin_file_times(&file_path);

    // Run touch with --time=use (alias for access)
    let output = Command::new(get_touch_binary_path())
        .arg("--time=use")
        .arg(file_path.to_str().unwrap())
        .output()
        .expect("Failed to execute touch command");

    assert!(
        output.status.success(),
        "Touch command failed: {:?}",
        output
    );

    // Check that only access time was updated
    let metadata = fs::metadata(&file_path).unwrap();
    let new_atime = FileTime::from_last_access_time(&metadata);
    let new_mtime = FileTime::from_last_modification_time(&metadata);

    assert_ne!(
        new_atime, original_atime,
        "Access time should have been updated"
    );
    assert_eq!(
        new_mtime, original_mtime,
        "Modification time should remain unchanged"
    );
}

#[test]
fn test_cli_default_behavior_updates_both() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test_cli_default.txt");
    fs::write(&file_path, "test content").unwrap();

    // Let any file-content scanner triggered by the write above finish, so a
    // scan does not bump the access time mid-test, then pin the timestamps.
    std::thread::sleep(std::time::Duration::from_millis(100));
    let (original_atime, original_mtime) = pin_file_times(&file_path);

    // Run touch without any time-specific options (should update both)
    let output = Command::new(get_touch_binary_path())
        .arg(file_path.to_str().unwrap())
        .output()
        .expect("Failed to execute touch command");

    assert!(
        output.status.success(),
        "Touch command failed: {:?}",
        output
    );

    // Check that both times were updated
    let metadata = fs::metadata(&file_path).unwrap();
    let new_atime = FileTime::from_last_access_time(&metadata);
    let new_mtime = FileTime::from_last_modification_time(&metadata);

    assert_ne!(
        new_atime, original_atime,
        "Access time should have been updated"
    );
    assert_ne!(
        new_mtime, original_mtime,
        "Modification time should have been updated"
    );
}

#[test]
fn test_cli_create_new_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("new_file.txt");

    // Ensure file doesn't exist
    assert!(!file_path.exists());

    // Run touch to create the file
    let output = Command::new(get_touch_binary_path())
        .arg("--time=access")
        .arg(file_path.to_str().unwrap())
        .output()
        .expect("Failed to execute touch command");

    assert!(
        output.status.success(),
        "Touch command failed: {:?}",
        output
    );

    // Check that file was created
    assert!(file_path.exists(), "File should have been created");
}

#[test]
fn test_cli_no_create_option() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("nonexistent_file.txt");

    // Ensure file doesn't exist
    assert!(!file_path.exists());

    // Run touch with --no-create option
    let output = Command::new(get_touch_binary_path())
        .arg("--no-create")
        .arg("--time=access")
        .arg(file_path.to_str().unwrap())
        .output()
        .expect("Failed to execute touch command");

    assert!(
        output.status.success(),
        "Touch command should succeed even when file doesn't exist with --no-create"
    );

    // Check that file was NOT created
    assert!(
        !file_path.exists(),
        "File should not have been created with --no-create option"
    );
}

#[test]
fn test_cli_ignore_flag_is_accepted() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test_ignore_flag.txt");
    fs::write(&file_path, "test content").unwrap();

    // Run touch with -f (ignore) flag - should succeed and work normally
    let output = Command::new(get_touch_binary_path())
        .arg("-f")
        .arg("--time=access")
        .arg(file_path.to_str().unwrap())
        .output()
        .expect("Failed to execute touch command");

    assert!(
        output.status.success(),
        "Touch command should accept -f flag without error: {:?}",
        output
    );
}

#[test]
fn test_cli_invalid_time_option() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test_invalid_time.txt");
    fs::write(&file_path, "test content").unwrap();

    // Run touch with invalid --time option
    let output = Command::new(get_touch_binary_path())
        .arg("--time=invalid")
        .arg(file_path.to_str().unwrap())
        .output()
        .expect("Failed to execute touch command");

    assert!(
        !output.status.success(),
        "Touch command should fail with invalid time option"
    );
}
