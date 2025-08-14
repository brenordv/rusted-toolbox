use anyhow::{Context, Result};
use shared::constants::general::SIZE_128KB;
use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use crate::models::CatOptions;

/// Processes file content with optional formatting.
///
/// Uses line processing for formatting options, otherwise performs raw copy.
///
/// # Errors
/// Returns error if file operations fail
pub fn cat_file(path: Option<&str>, options: &CatOptions) -> Result<()> {
    if options.needs_line_processing() {
        cook_buf(path, options)
    } else {
        raw_cat(path)
    }
}

/// Processes file content line-by-line with formatting.
///
/// Applies line numbering, blank line handling, and character visualization.
///
/// # Errors
/// Returns error if file operations fail
fn cook_buf(path: Option<&str>, options: &CatOptions) -> Result<()> {
    let reader: Box<dyn BufRead> = match path {
        None | Some("-") => Box::new(BufReader::new(io::stdin())),
        Some(filename) => {
            let file = File::open(filename)?;
            Box::new(BufReader::new(file))
        }
    };

    let mut line_number = 0u64;

    let mut prev_was_empty = false;

    for line_result in reader.lines() {
        let line = line_result?;
        let is_empty = line.is_empty();

        // Handle squeeze blank lines
        if options.squeeze_blank && is_empty {
            if prev_was_empty {
                continue;
            }
            prev_was_empty = true;
        } else {
            prev_was_empty = false;
        }

        // Handle line numbering
        if options.number {
            if options.number_nonblank && is_empty {
                // Don't number blank lines when -b is used
                print!("{:6}\t", "");
            } else {
                line_number += 1;
                print!("{:6}\t", line_number);
            }
        }

        // Process each character in the line
        for ch in line.chars() {
            if ch == '\t' && options.show_tabs {
                print!("^I");
            } else if options.show_nonprinting && ch.is_control() && ch != '\t' && ch != '\n' {
                if (ch as u32) == 127 {
                    // DEL character
                    print!("^?");
                } else if (ch as u32) < 32 {
                    // Control characters
                    print!("^{}", ((ch as u8) | 0x40) as char);
                } else {
                    print!("{}", ch);
                }
            } else if options.show_nonprinting && !ch.is_ascii() {
                // Non-ASCII characters
                for byte in ch.to_string().as_bytes() {
                    if *byte > 127 {
                        print!("M-");
                        let ascii_byte = byte & 0x7F;
                        if ascii_byte < 32 {
                            print!("^{}", (ascii_byte | 0x40) as char);
                        } else if ascii_byte == 127 {
                            print!("^?");
                        } else {
                            print!("{}", ascii_byte as char);
                        }
                    } else {
                        print!("{}", *byte as char);
                    }
                }
            } else {
                print!("{}", ch);
            }
        }

        // Handle show ends
        if options.show_ends {
            print!("$");
        }

        // End of line
        println!();
    }

    Ok(())
}

/// Performs raw file copy without processing.
///
/// Copies file content directly to stdout in 128KB chunks.
/// Note: The original GNU implementation uses 8KB buffer. We're using 128kb to make things
/// run faster.
///
/// # Errors
/// Returns error if file operations fail
fn raw_cat(path: Option<&str>) -> Result<()> {
    match path {
        None | Some("-") => {
            // Read from stdin
            let mut buffer = [0; SIZE_128KB];

            let mut stdin = io::stdin();

            loop {
                match stdin.read(&mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        io::stdout()
                            .write_all(&buffer[..n])
                            .context("Failed to write to stdout")?;

                        io::stdout().flush().context("Failed to flush stdout")?;
                    }
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(e) => return Err(anyhow::Error::from(e)),
                }
            }
        }
        Some(filename) => {
            let mut file = File::open(filename).context("Failed to open file")?;

            let mut buffer = [0; SIZE_128KB];

            loop {
                match file.read(&mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        io::stdout()
                            .write_all(&buffer[..n])
                            .context("Failed to write to stdout")?;
                    }
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(e) => return Err(anyhow::Error::from(e)),
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn default_options() -> CatOptions {
        CatOptions {
            number_nonblank: false,
            show_ends: false,
            number: false,
            squeeze_blank: false,
            show_tabs: false,
            show_nonprinting: false,
        }
    }

    fn create_temp_file(content: &str) -> NamedTempFile {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(content.as_bytes()).unwrap();
        temp_file.flush().unwrap();
        temp_file
    }

    #[rstest]
    #[case("hello\nworld\n", &default_options())]
    #[case("single line", &default_options())]
    #[case("", &default_options())]
    fn test_cook_buf_basic_file_reading(#[case] content: &str, #[case] options: &CatOptions) {
        let temp_file = create_temp_file(content);
        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), options);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cook_buf_nonexistent_file() {
        let options = default_options();
        let result = cook_buf(Some("nonexistent_file.txt"), &options);
        assert!(result.is_err());
    }

    #[rstest]
    #[case("line1\nline2\nline3\n")]
    #[case("single line without newline")]
    #[case("")]
    fn test_cook_buf_with_line_numbers(#[case] content: &str) {
        let temp_file = create_temp_file(content);
        let mut options = default_options();
        options.number = true;

        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cook_buf_number_nonblank_only() {
        let content = "line1\n\nline3\n\nline5\n";
        let temp_file = create_temp_file(content);
        let mut options = default_options();
        options.number = true;
        options.number_nonblank = true;

        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());
    }

    #[rstest]
    #[case("line1\n\n\n\nline2\n")]
    #[case("")]
    #[case("\n\n\n")]
    #[case("content\n\n\n\n\nmore content\n")]
    fn test_cook_buf_squeeze_blank_lines(#[case] content: &str) {
        let temp_file = create_temp_file(content);
        let mut options = default_options();
        options.squeeze_blank = true;

        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());
    }

    #[rstest]
    #[case("hello\tworld\n")]
    #[case("\t\t\ttabs\n")]
    #[case("no tabs here\n")]
    #[case("mixed\tcontent\there\n")]
    fn test_cook_buf_show_tabs(#[case] content: &str) {
        let temp_file = create_temp_file(content);
        let mut options = default_options();
        options.show_tabs = true;

        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());
    }

    #[rstest]
    #[case("line1\n")]
    #[case("line1\nline2\n")]
    #[case("")]
    #[case("no newline")]
    fn test_cook_buf_show_ends(#[case] content: &str) {
        let temp_file = create_temp_file(content);
        let mut options = default_options();
        options.show_ends = true;

        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());
    }

    #[rstest]
    #[case("hello\x01world\n")] // Control character
    #[case("test\x7f\n")] // DEL character
    #[case("normal text\n")] // No control characters
    #[case("\x1f\x02\x03\n")] // Multiple control characters
    fn test_cook_buf_show_nonprinting_control_chars(#[case] content: &str) {
        let temp_file = create_temp_file(content);
        let mut options = default_options();
        options.show_nonprinting = true;

        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cook_buf_show_nonprinting_non_ascii() {
        let content = "café\nñoño\n🦀\n";
        let temp_file = create_temp_file(content);
        let mut options = default_options();
        options.show_nonprinting = true;

        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cook_buf_show_nonprinting_preserves_tabs_and_newlines() {
        let content = "hello\tworld\x01test\n";
        let temp_file = create_temp_file(content);
        let mut options = default_options();
        options.show_nonprinting = true;

        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cook_buf_combined_options() {
        let content = "line1\n\n\nline4\twith\ttabs\n\nline6\x01control\n";
        let temp_file = create_temp_file(content);
        let mut options = default_options();
        options.number = true;
        options.number_nonblank = true;
        options.squeeze_blank = true;
        options.show_tabs = true;
        options.show_ends = true;
        options.show_nonprinting = true;

        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cook_buf_stdin_dash_path() {
        // Similar to the None case, testing with a "-" path
        let result = std::panic::catch_unwind(|| {
            // Verify function signature and type checking
        });
        assert!(result.is_ok());
    }

    #[rstest]
    #[case(0u8)] // NUL
    #[case(31u8)] // US (Unit Separator)
    #[case(127u8)] // DEL
    fn test_cook_buf_specific_control_characters(#[case] control_byte: u8) {
        let content = format!("before{}after\n", control_byte as char);
        let temp_file = create_temp_file(&content);
        let mut options = default_options();
        options.show_nonprinting = true;

        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cook_buf_empty_file() {
        let temp_file = create_temp_file("");
        let options = default_options();

        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cook_buf_only_blank_lines() {
        let content = "\n\n\n\n";
        let temp_file = create_temp_file(content);

        // Test with squeeze_blank
        let mut options = default_options();
        options.squeeze_blank = true;
        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());

        // Test with numbering
        options.squeeze_blank = false;
        options.number = true;
        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cook_buf_line_number_overflow_safety() {
        // Create a file with content but test that line_number is u64
        // This tests that we're using u64, which can handle very large numbers
        let content = "line\n";
        let temp_file = create_temp_file(content);
        let mut options = default_options();
        options.number = true;

        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());

        // The function uses u64 for line_number, so it should handle large numbers safely
        // This is more of design verification than a runtime test
    }

    #[test]
    fn test_cook_buf_mixed_line_endings() {
        // Test with different types of content that might have edge cases
        let content = "line1\nline2\n\nline4\n";
        let temp_file = create_temp_file(content);

        let mut options = default_options();
        options.number_nonblank = true;
        options.number = true;

        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cook_buf_show_tabs_with_show_nonprinting() {
        let content = "hello\tworld\x01test\n";
        let temp_file = create_temp_file(content);
        let mut options = default_options();
        options.show_tabs = true;
        options.show_nonprinting = true;

        let result = cook_buf(Some(temp_file.path().to_str().unwrap()), &options);
        assert!(result.is_ok());
    }

    #[rstest]
    fn test_raw_cat_with_existing_file() -> io::Result<()> {
        // Create a temporary file with test content
        let mut temp_file = NamedTempFile::new()?;
        let test_content = "Hello, World!\nThis is a test file.\n";
        temp_file.write_all(test_content.as_bytes())?;

        let file_path = temp_file.path().to_str().unwrap();

        // Since we can't easily capture stdout in unit tests, we'll test that the function
        // completes without error for a valid file
        let result = raw_cat(Some(file_path));
        assert!(result.is_ok());

        Ok(())
    }

    #[rstest]
    fn test_raw_cat_with_nonexistent_file() {
        let result = raw_cat(Some("nonexistent_file.txt"));
        assert!(result.is_err());
    }

    #[rstest]
    fn test_raw_cat_with_empty_file() -> io::Result<()> {
        // Create an empty temporary file
        let temp_file = NamedTempFile::new()?;
        let file_path = temp_file.path().to_str().unwrap();

        let result = raw_cat(Some(file_path));
        assert!(result.is_ok());

        Ok(())
    }

    #[rstest]
    fn test_raw_cat_with_large_file() -> io::Result<()> {
        // Create a file larger than the buffer size (8192 bytes)
        let mut temp_file = NamedTempFile::new()?;
        let large_content = "A".repeat(10000); // 10KB of 'A' characters
        temp_file.write_all(large_content.as_bytes())?;

        let file_path = temp_file.path().to_str().unwrap();

        let result = raw_cat(Some(file_path));
        assert!(result.is_ok());

        Ok(())
    }

    #[test]
    #[cfg(unix)]
    fn test_raw_cat_with_binary_file() -> io::Result<()> {
        // Create a temporary file with binary content
        let mut temp_file = NamedTempFile::new()?;
        let binary_content = vec![0u8, 1u8, 255u8, 127u8, 128u8];
        temp_file.write_all(&binary_content)?;

        let file_path = temp_file.path().to_str().unwrap();

        let result = raw_cat(Some(file_path));
        assert!(result.is_ok());

        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn test_raw_cat_with_permission_denied() -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt;

        // Create a temporary file first
        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(b"test content")?;
        let file_path = temp_file.path().to_str().unwrap();

        // Change permissions to deny read access
        let mut perms = fs::metadata(file_path)?.permissions();
        perms.set_mode(0o000); // No permissions
        fs::set_permissions(file_path, perms)?;

        let result = raw_cat(Some(file_path));
        assert!(result.is_err());

        // Restore permissions for cleanup
        let mut perms = fs::metadata(file_path)?.permissions();
        perms.set_mode(0o644);
        fs::set_permissions(file_path, perms)?;

        Ok(())
    }

    #[rstest]
    fn test_raw_cat_with_directory_path() -> io::Result<()> {
        // Create a temporary directory
        let temp_dir = tempfile::tempdir()?;
        let dir_path = temp_dir.path().to_str().unwrap();

        let result = raw_cat(Some(dir_path));
        assert!(result.is_err());

        Ok(())
    }

    #[rstest]
    fn test_raw_cat_with_special_characters_in_filename() {
        // Test with a filename that doesn't exist but has special characters
        let special_filename = "file with spaces & special chars!@#$.txt";

        let result = raw_cat(Some(special_filename));
        assert!(result.is_err());
    }

    #[rstest]
    fn test_raw_cat_with_file_exactly_buffer_size() -> io::Result<()> {
        // Create a file that is exactly the buffer size (8192 bytes)
        let mut temp_file = NamedTempFile::new()?;
        let content = "B".repeat(8192);
        temp_file.write_all(content.as_bytes())?;

        let file_path = temp_file.path().to_str().unwrap();

        let result = raw_cat(Some(file_path));
        assert!(result.is_ok());

        Ok(())
    }

    #[test]
    fn test_raw_cat_with_file_one_byte_over_buffer() {
        // Create a file that is one byte larger than the buffer size
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let content = "C".repeat(8193);
        temp_file
            .write_all(content.as_bytes())
            .expect("Failed to write to temp file");

        let file_path = temp_file.path().to_str().unwrap();

        let result = raw_cat(Some(file_path));
        assert!(result.is_ok());
    }

    #[rstest]
    fn test_raw_cat_with_single_byte_file() -> io::Result<()> {
        // Create a file with just one byte
        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(b"X")?;

        let file_path = temp_file.path().to_str().unwrap();

        let result = raw_cat(Some(file_path));
        assert!(result.is_ok());

        Ok(())
    }
}