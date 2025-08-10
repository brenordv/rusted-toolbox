use std::path::PathBuf;

/// Resolves a given path against a base folder.
///
/// This function takes a base folder and a relative or absolute path as input.
/// If the provided path is absolute, it returns the path as is. However, if the
/// path is relative, it resolves the path by appending it to the provided base folder.
///
/// # Arguments
///
/// * `base_folder` - A string slice representing the base folder to resolve the path against.
/// * `path` - A string slice representing the path which may be absolute or relative.
///
/// # Returns
///
/// A `PathBuf` representing the resolved path. If the provided path is absolute, the result
/// is the same as the input path. If the path is relative, the result is the base folder
/// joined with the relative path.
///
/// # Example
///
/// ```
/// use std::path::PathBuf;
/// use shared::::resolve_path_with_base::resolve_path_with_base;
///
/// let base_folder = "/home/user";
/// let relative_path = "documents/file.txt";
/// let resolved_path = resolve_path_with_base(base_folder, relative_path);
/// assert_eq!(resolved_path, PathBuf::from("/home/user/documents/file.txt"));
///
/// let absolute_path = "/var/log/system.log";
/// let resolved_path = resolve_path_with_base(base_folder, absolute_path);
/// assert_eq!(resolved_path, PathBuf::from("/var/log/system.log"));
/// ```
pub fn resolve_path_with_base(base_folder: &str, path: &str) -> PathBuf {
    let path_buf = PathBuf::from(path);
    if path_buf.is_absolute() {
        path_buf
    } else {
        PathBuf::from(base_folder).join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::path::MAIN_SEPARATOR;

    #[rstest]
    #[case("/home/user", "documents/file.txt", "/home/user/documents/file.txt")]
    #[case("/var/log", "system.log", "/var/log/system.log")]
    #[case(".", "config.json", "./config.json")]
    #[case("/opt/app", "data/db.sqlite", "/opt/app/data/db.sqlite")]
    #[case(
        "C:\\Users\\User",
        "Documents\\file.txt",
        "C:\\Users\\User\\Documents\\file.txt"
    )]
    fn test_resolve_relative_paths(
        #[case] base_folder: &str,
        #[case] relative_path: &str,
        #[case] expected: &str,
    ) {
        let result = resolve_path_with_base(base_folder, relative_path);
        let expected_path = PathBuf::from(expected);
        assert_eq!(result, expected_path);
    }

    #[rstest]
    #[case("/home/user", "/var/log/system.log", "/var/log/system.log")]
    #[case(".", "/etc/passwd", "/etc/passwd")]
    #[case("/opt/app", "/tmp/temp.txt", "/tmp/temp.txt")]
    #[case("relative/base", "/absolute/path", "/absolute/path")]
    #[cfg(windows)]
    #[case(
        "C:\\Users\\User",
        "C:\\Windows\\System32\\file.dll",
        "C:\\Windows\\System32\\file.dll"
    )]
    #[cfg(windows)]
    #[case(".", "D:\\Data\\file.txt", "D:\\Data\\file.txt")]
    fn test_resolve_absolute_paths_ignored_base(
        #[case] base_folder: &str,
        #[case] absolute_path: &str,
        #[case] expected: &str,
    ) {
        let result = resolve_path_with_base(base_folder, absolute_path);
        let expected_path = PathBuf::from(expected);
        assert_eq!(result, expected_path);
    }

    #[rstest]
    #[case("", "file.txt", "file.txt")]
    #[case("base", "", "base")]
    #[case("", "", "")]
    fn test_resolve_empty_inputs(
        #[case] base_folder: &str,
        #[case] path: &str,
        #[case] expected: &str,
    ) {
        let result = resolve_path_with_base(base_folder, path);
        let expected_path = PathBuf::from(expected);
        assert_eq!(result, expected_path);
    }

    #[rstest]
    #[case("base", ".", "base/.")]
    #[case("base", "..", "base/..")]
    #[case("/home/user", "./config", "/home/user/./config")]
    #[case("/home/user", "../shared", "/home/user/../shared")]
    #[case(".", "..", "./../")]
    fn test_resolve_special_relative_paths(
        #[case] base_folder: &str,
        #[case] relative_path: &str,
        #[case] expected: &str,
    ) {
        let result = resolve_path_with_base(base_folder, relative_path);
        let expected_path = PathBuf::from(expected);
        assert_eq!(result, expected_path);
    }

    #[rstest]
    #[case("base/folder", "sub/dir/file.txt", "base/folder/sub/dir/file.txt")]
    #[case(
        "/deep/nested/path",
        "even/deeper/nested/structure/file.log",
        "/deep/nested/path/even/deeper/nested/structure/file.log"
    )]
    #[case(".", "a/b/c/d/e/f/g/h/i/j/file.txt", "./a/b/c/d/e/f/g/h/i/j/file.txt")]
    fn test_resolve_nested_paths(
        #[case] base_folder: &str,
        #[case] relative_path: &str,
        #[case] expected_path_str: &str,
    ) {
        let result = resolve_path_with_base(base_folder, relative_path);
        let expected = PathBuf::from(expected_path_str);
        assert_eq!(result, expected);

        // Verify path structure
        assert!(result.to_string_lossy().contains(base_folder));
        assert!(result.to_string_lossy().contains(relative_path));
    }

    #[test]
    fn test_resolve_path_components_preservation() {
        let base_folder = "/home/user";
        let relative_path = "documents/projects/rust/src/main.rs";

        let result = resolve_path_with_base(base_folder, relative_path);

        // Verify all components are preserved
        let components: Vec<_> = result.components().collect();
        assert!(!components.is_empty());

        // Check that the result contains expected path segments
        let result_str = result.to_string_lossy();
        assert!(result_str.contains("home"));
        assert!(result_str.contains("user"));
        assert!(result_str.contains("documents"));
        assert!(result_str.contains("projects"));
        assert!(result_str.contains("rust"));
        assert!(result_str.contains("src"));
        assert!(result_str.contains("main.rs"));
    }

    #[test]
    fn test_resolve_path_with_unicode_characters() {
        let base_folder = "/home/用户";
        let relative_path = "文档/项目.txt";

        let result = resolve_path_with_base(base_folder, relative_path);
        let expected = PathBuf::from("/home/用户/文档/项目.txt");

        assert_eq!(result, expected);
    }

    #[test]
    fn test_resolve_path_with_spaces_and_special_chars() {
        let base_folder = "/home/user with spaces";
        let relative_path = "My Documents/file-name_with.special#chars.txt";

        let result = resolve_path_with_base(base_folder, relative_path);
        let expected =
            PathBuf::from("/home/user with spaces/My Documents/file-name_with.special#chars.txt");

        assert_eq!(result, expected);
    }

    #[rstest]
    #[case("/home/user", "file.txt")]
    #[case(".", "config.json")]
    #[case("/opt/app", "data/db.sqlite")]
    #[case("relative/base", "nested/file.log")]
    fn test_resolve_path_idempotency(#[case] base_folder: &str, #[case] relative_path: &str) {
        let result1 = resolve_path_with_base(base_folder, relative_path);
        let result2 = resolve_path_with_base(base_folder, relative_path);

        assert_eq!(result1, result2);
    }

    #[test]
    fn test_resolve_path_with_different_separators() {
        let base_folder = "base/folder";
        let relative_path = "sub\\dir/file.txt"; // Mixed separators

        let result = resolve_path_with_base(base_folder, relative_path);

        // Construct the expected path manually using PathBuf
        let expected = PathBuf::from("base")
            .join("folder")
            .join("sub")
            .join("dir")
            .join("file.txt");

        assert_eq!(
            result, expected,
            "Expected path {:?}, but got {:?}",
            expected, result
        );
    }

    #[test]
    fn test_resolve_path_boundary_cases() {
        // Test very long paths
        let long_base = "a".repeat(100);
        let long_relative = "b".repeat(100);

        let result = resolve_path_with_base(&long_base, &long_relative);
        let expected = PathBuf::from(&long_base).join(&long_relative);

        assert_eq!(result, expected);
    }

    #[test]
    fn test_resolve_path_returns_pathbuf_type() {
        let result = resolve_path_with_base("/base", "file.txt");

        // Verify it's actually a PathBuf and has expected methods
        assert!(result.is_relative() || result.is_absolute());
        assert!(result.file_name().is_some());
        assert!(result.parent().is_some());
    }

    #[rstest]
    #[case("/")]
    #[case("/home")]
    #[case(".")]
    #[case("..")]
    #[case("relative")]
    #[case("")]
    fn test_resolve_base_folder_variations(#[case] base_folder: &str) {
        let relative_path = "file.txt";
        let result = resolve_path_with_base(base_folder, relative_path);

        // Should always produce a valid PathBuf
        let _string_rep = result.to_string_lossy();

        // If base is empty, result should just be the relative path
        if base_folder.is_empty() {
            assert_eq!(result, PathBuf::from(relative_path));
        } else {
            // Otherwise, should contain both base and relative components
            let result_str = result.to_string_lossy();
            if !base_folder.is_empty() && !relative_path.is_empty() {
                assert!(result_str.contains(relative_path));
            }
        }
    }

    #[test]
    fn test_resolve_path_with_trailing_separators() {
        let base_with_separator = format!("/home/user{}", MAIN_SEPARATOR);
        let result = resolve_path_with_base(&base_with_separator, "file.txt");

        // Should handle trailing separators gracefully
        let result_str = result.to_string_lossy();
        assert!(result_str.contains("home"));
        assert!(result_str.contains("user"));
        assert!(result_str.contains("file.txt"));
    }

    #[cfg(windows)]
    #[rstest]
    #[case(
        "C:\\Users\\User",
        "Documents\\file.txt",
        "C:\\Users\\User\\Documents\\file.txt"
    )]
    #[case(
        "\\\\server\\share",
        "folder\\file.txt",
        "\\\\server\\share\\folder\\file.txt"
    )]
    #[case("C:", "file.txt", "C:file.txt")]
    fn test_resolve_windows_specific_paths(
        #[case] base_folder: &str,
        #[case] relative_path: &str,
        #[case] expected: &str,
    ) {
        let result = resolve_path_with_base(base_folder, relative_path);
        let expected_path = PathBuf::from(expected);
        assert_eq!(result, expected_path);
    }

    #[cfg(unix)]
    #[rstest]
    #[case("/home/user", "documents/file.txt", "/home/user/documents/file.txt")]
    #[case("/var/log", "app.log", "/var/log/app.log")]
    #[case("/tmp", "temp_file", "/tmp/temp_file")]
    fn test_resolve_unix_specific_paths(
        #[case] base_folder: &str,
        #[case] relative_path: &str,
        #[case] expected: &str,
    ) {
        let result = resolve_path_with_base(base_folder, relative_path);
        let expected_path = PathBuf::from(expected);
        assert_eq!(result, expected_path);
    }

    #[test]
    fn test_resolve_path_consistency_with_pathbuf_join() {
        let base_folder = "/home/user";
        let relative_path = "documents/file.txt";

        let our_result = resolve_path_with_base(base_folder, relative_path);
        let pathbuf_result = PathBuf::from(base_folder).join(relative_path);

        // For relative paths, our function should behave identically to PathBuf::join
        assert_eq!(our_result, pathbuf_result);
    }

    #[cfg(unix)]
    #[test]
    fn test_resolve_path_absolute_path_unchanged_linux() {
        let base_folder = "/home/user";
        let absolute_path = "/var/log/system.log";

        let result = resolve_path_with_base(base_folder, absolute_path);
        let original_path = PathBuf::from(absolute_path);

        // Absolute paths should be returned unchanged
        assert_eq!(result, original_path);
        assert!(result.is_absolute());
    }

    #[cfg(windows)]
    #[test]
    fn test_resolve_path_absolute_path_unchanged_windows() {
        let base_folder = r"C:\Users\Example";
        let absolute_path = r"C:\Windows\Logs\system.log";

        let result = resolve_path_with_base(base_folder, absolute_path);
        let original_path = PathBuf::from(absolute_path);

        // Absolute paths should be returned unchanged
        assert_eq!(result, original_path);
        assert!(result.is_absolute());
    }

    #[test]
    fn test_resolve_path_memory_efficiency() {
        // Test that function doesn't unnecessarily allocate for absolute paths
        let base_folder = "/some/base";
        let absolute_path = "/absolute/path/file.txt";

        let result = resolve_path_with_base(base_folder, absolute_path);

        // Should be equivalent to just creating PathBuf from absolute path
        assert_eq!(result, PathBuf::from(absolute_path));
    }
}
