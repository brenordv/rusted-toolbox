use std::env;
use std::path::PathBuf;

/// Retrieves the current working directory of the process.
///
/// # Returns
/// * `PathBuf` - A `PathBuf` representing the current working directory.
///
/// # Behavior
/// If the current working directory cannot be determined (e.g., due to an
/// error), the function will fallback to returning a `PathBuf` representing
/// the current directory (`.`).
///
/// # Panics
/// This function does not panic directly. However, in case of an error
/// retrieving the current directory, it uses a fallback mechanism and defaults
/// to `PathBuf::from(".")`.
///
/// # Examples
/// ```
/// use std::path::PathBuf;
/// use rusted_toolbox::shared::system::get_current_working_dir::get_current_working_dir;
///
/// let cwd: PathBuf = get_current_working_dir();
/// println!("Current working directory: {:?}", cwd);
/// ```
pub fn get_current_working_dir() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn test_get_current_working_dir_matches_env_current_dir() {
        let result = get_current_working_dir();

        // Under normal circumstances, our function should return the same as env::current_dir()
        if let Ok(expected) = env::current_dir() {
            assert_eq!(result, expected);
        } else {
            // If env::current_dir() fails, our function should return "."
            assert_eq!(result, PathBuf::from("."));
        }
    }

    #[test]
    fn test_get_current_working_dir_is_valid_directory() {
        let result = get_current_working_dir();
        assert!(result.exists(), "Returned path should exist: {:?}", result);
        assert!(
            result.is_dir(),
            "Returned path should be a directory: {:?}",
            result
        );
    }

    #[test]
    fn test_get_current_working_dir_with_changed_directory() {
        // Create a temporary directory to change into
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_path_buf();

        // Store original directory
        let original_dir = env::current_dir().expect("Failed to get original directory");

        // Change to temp directory
        env::set_current_dir(&temp_path).expect("Failed to change directory");

        // Test our function
        let result = get_current_working_dir();

        // Restore original directory before assertions (cleanup)
        env::set_current_dir(original_dir).expect("Failed to restore original directory");

        // Verify the result matches the temp directory we changed to
        assert_eq!(result, temp_path);
    }

    #[test]
    fn test_get_current_working_dir_returns_absolute_path_when_possible() {
        let result = get_current_working_dir();

        // When env::current_dir() succeeds, it always returns an absolute path
        // Our function should preserve this behavior
        if result != PathBuf::from(".") {
            assert!(
                result.is_absolute(),
                "Should return absolute path when possible: {:?}",
                result
            );
        }
    }

    #[test]
    fn test_get_current_working_dir_fallback_behavior() {
        // This test verifies the fallback behavior, though it's hard to trigger
        // env::current_dir() failure in a controlled way. We'll test the logic indirectly.
        let result = get_current_working_dir();

        // The function should never panic and always return a valid PathBuf
        assert!(result == PathBuf::from(".") || result.is_absolute());
    }

    #[test]
    fn test_get_current_working_dir_consistent_results() {
        // Multiple calls should return the same result (assuming no directory changes)
        let result1 = get_current_working_dir();
        let result2 = get_current_working_dir();

        assert_eq!(
            result1, result2,
            "Multiple calls should return consistent results"
        );
    }

    #[test]
    fn test_get_current_working_dir_handles_root_directory() {
        // Store original directory
        let original_dir = env::current_dir().expect("Failed to get original directory");

        // Try to change to root directory (this may fail on some systems/permissions)
        let root_path = if cfg!(windows) {
            PathBuf::from("C:\\")
        } else {
            PathBuf::from("/")
        };

        if env::set_current_dir(&root_path).is_ok() {
            let result = get_current_working_dir();

            // Restore original directory
            env::set_current_dir(original_dir).expect("Failed to restore original directory");

            // Should handle root directory correctly
            assert_eq!(result, root_path);
        } else {
            // If we can't change to root, just restore and pass
            env::set_current_dir(original_dir).expect("Failed to restore original directory");
        }
    }

    #[test]
    fn test_get_current_working_dir_with_nested_directories() {
        // Create nested temporary directories
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let nested_path = temp_dir.path().join("level1").join("level2").join("level3");
        std::fs::create_dir_all(&nested_path).expect("Failed to create nested directories");

        // Store original directory
        let original_dir = env::current_dir().expect("Failed to get original directory");

        // Change to nested directory
        env::set_current_dir(&nested_path).expect("Failed to change to nested directory");

        // Test our function
        let result = get_current_working_dir();

        // Restore original directory
        env::set_current_dir(original_dir).expect("Failed to restore original directory");

        // Verify the result
        assert_eq!(result, nested_path);
        assert!(result.is_absolute());
    }

    #[test]
    fn test_get_current_working_dir_path_components() {
        let result = get_current_working_dir();

        if result != PathBuf::from(".") {
            // Should be able to iterate over components without panicking
            let components: Vec<_> = result.components().collect();
            assert!(
                !components.is_empty(),
                "Path should have at least one component"
            );

            // Should be able to get parent (except for root)
            if result.parent().is_some() {
                assert!(
                    result.parent().unwrap().exists() || result.parent().unwrap() == Path::new("")
                );
            }
        }
    }

    #[test]
    fn test_get_current_working_dir_string_conversion() {
        let result = get_current_working_dir();

        // Should be able to convert to string representations
        let _as_str = result.to_string_lossy();
        let _display = format!("{}", result.display());

        // These operations should never panic
        assert!(true); // If we get here without panicking, test passes
    }
}
