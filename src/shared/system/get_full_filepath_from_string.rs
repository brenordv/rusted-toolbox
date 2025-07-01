use crate::shared::system::get_current_working_dir::get_current_working_dir;
use std::path::{Path, PathBuf};
pub fn get_full_filepath_from_string(file: &String) -> PathBuf {
    let path = Path::new(file);
    if path.is_absolute() {
        // If it's already an absolute path, convert it to PathBuf and return
        path.to_path_buf()
    } else {
        // If it's a relative path, join it with the current working directory
        let cwd = get_current_working_dir();
        cwd.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;
    use std::path::{Path, PathBuf};

    #[rstest]
    #[case("/absolute/path/file.txt")]
    #[case("/")]
    #[case("/home/user/document.pdf")]
    #[case("/tmp/test.log")]
    #[cfg(unix)]
    fn test_absolute_unix_paths(#[case] absolute_path: &str) {
        let input = absolute_path.to_string();
        let result = get_full_filepath_from_string(&input);

        assert!(result.is_absolute());
        assert_eq!(result, PathBuf::from(absolute_path));
    }

    #[rstest]
    #[case("C:\\absolute\\path\\file.txt")]
    #[case("C:\\")]
    #[case("D:\\Users\\user\\document.pdf")]
    #[case("Z:\\temp\\test.log")]
    #[cfg(windows)]
    fn test_absolute_windows_paths(#[case] absolute_path: &str) {
        let input = absolute_path.to_string();
        let result = get_full_filepath_from_string(&input);

        assert!(result.is_absolute());
        assert_eq!(result, PathBuf::from(absolute_path));
    }

    #[rstest]
    #[case("relative/path/file.txt")]
    #[case("file.txt")]
    #[case("./file.txt")]
    #[case("../parent/file.txt")]
    #[case("subdirectory/another/file.log")]
    fn test_relative_paths(#[case] relative_path: &str) {
        let input = relative_path.to_string();
        let result = get_full_filepath_from_string(&input);
        let expected_cwd = get_current_working_dir();
        let expected = expected_cwd.join(relative_path);

        assert!(result.is_absolute());
        assert_eq!(result, expected);
    }

    #[test]
    fn test_empty_string() {
        let empty_path = "";
        let input = empty_path.to_string();
        let result = get_full_filepath_from_string(&input);
        let expected_cwd = get_current_working_dir();
        let expected = expected_cwd.join("");

        assert_eq!(result, expected);
    }

    #[rstest]
    #[case(".")]
    #[case("..")]
    fn test_current_and_parent_directory_references(#[case] path: &str) {
        let input = path.to_string();
        let result = get_full_filepath_from_string(&input);
        let expected_cwd = get_current_working_dir();
        let expected = expected_cwd.join(path);

        assert!(result.is_absolute());
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case("file with spaces.txt")]
    #[case("file-with-dashes.log")]
    #[case("file_with_underscores.data")]
    #[case("file.with.dots.config")]
    #[case("file123numbers.txt")]
    fn test_special_characters_in_filenames(#[case] filename: &str) {
        let input = filename.to_string();
        let result = get_full_filepath_from_string(&input);
        let expected_cwd = get_current_working_dir();
        let expected = expected_cwd.join(filename);

        assert!(result.is_absolute());
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case("a/very/deep/nested/directory/structure/file.txt")]
    fn test_deeply_nested_relative_paths(#[case] nested_path: &str) {
        let input = nested_path.to_string();
        let result = get_full_filepath_from_string(&input);
        let expected_cwd = get_current_working_dir();
        let expected = expected_cwd.join(nested_path);

        assert!(result.is_absolute());
        assert_eq!(result, expected);

        // Verify the path components are preserved
        let result_components: Vec<_> = result.components().collect();
        let expected_components: Vec<_> = expected.components().collect();
        assert_eq!(result_components, expected_components);
    }

    #[test]
    fn test_function_does_not_modify_input() {
        let original = "test/file.txt".to_string();
        let input_copy = original.clone();
        let _result = get_full_filepath_from_string(&input_copy);

        // Verify the input string was not modified
        assert_eq!(input_copy, original);
    }

    #[test]
    fn test_result_is_always_pathbuf() {
        let relative_input = "relative.txt".to_string();
        let relative_result = get_full_filepath_from_string(&relative_input);
        assert!(matches!(relative_result, PathBuf { .. }));

        #[cfg(unix)]
        {
            let absolute_input = "/absolute.txt".to_string();
            let absolute_result = get_full_filepath_from_string(&absolute_input);
            assert!(matches!(absolute_result, PathBuf { .. }));
        }

        #[cfg(windows)]
        {
            let absolute_input = "C:\\absolute.txt".to_string();
            let absolute_result = get_full_filepath_from_string(&absolute_input);
            assert!(matches!(absolute_result, PathBuf { .. }));
        }
    }

    #[rstest]
    #[case("./././file.txt")]
    #[case("../../../file.txt")]
    #[case("dir/../file.txt")]
    fn test_path_with_redundant_components(#[case] path_with_dots: &str) {
        let input = path_with_dots.to_string();
        let result = get_full_filepath_from_string(&input);
        let expected_cwd = get_current_working_dir();
        let expected = expected_cwd.join(path_with_dots);

        assert!(result.is_absolute());
        assert_eq!(result, expected);
    }

    #[test]
    fn test_consistency_with_path_new() {
        let test_paths = vec![
            "relative.txt",
            "dir/file.txt",
            "./current.txt",
            "../parent.txt",
        ];

        for path_str in test_paths {
            let input = path_str.to_string();
            let result = get_full_filepath_from_string(&input);
            let path_from_new = Path::new(path_str);

            // Verify our function behaves consistently with Path::new for relative paths
            assert_eq!(path_from_new.is_absolute(), false);
            assert!(result.is_absolute()); // Our function should always return absolute paths for relative inputs
        }
    }

    #[rstest]
    #[case("very_long_filename_that_exceeds_typical_limits_but_should_still_work_correctly_with_our_function.txt")]
    fn test_long_filename(#[case] long_filename: &str) {
        let input = long_filename.to_string();
        let result = get_full_filepath_from_string(&input);
        let expected_cwd = get_current_working_dir();
        let expected = expected_cwd.join(long_filename);

        assert!(result.is_absolute());
        assert_eq!(result, expected);

        // Verify the filename is preserved correctly
        assert_eq!(result.file_name().unwrap(), long_filename);
    }
}
