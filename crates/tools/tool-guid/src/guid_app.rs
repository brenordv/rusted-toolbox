use anyhow::{Context, Result};
use common_cli::broken_pipe::write_out;
use common_utils_ext::copy_string_to_clipboard::copy_to_clipboard;
use common_utils_ext::new_guid::new_guid;
use std::io::Write;

/// Writes `target_guid_count` freshly generated guids to `output`, one per line.
///
/// # Errors
/// Fails with the [`common_cli::broken_pipe::BrokenPipe`] marker when the
/// consumer closes the pipe, or with the underlying I/O error for any other
/// write failure.
pub fn generate_multiple_guid(target_guid_count: usize, output: &mut impl Write) -> Result<()> {
    for _ in 0..target_guid_count {
        write_out(output, format!("{}\n", create_guid(false)).as_bytes())?;
    }

    Ok(())
}

/// Returns the all-zeros guid when `empty_guid` is set, otherwise a new uuid-v4.
pub fn create_guid(empty_guid: bool) -> String {
    if empty_guid {
        return "00000000-0000-0000-0000-000000000000".to_string();
    }

    new_guid()
}

/// Copies the guid to the system clipboard.
///
/// # Errors
/// Fails when the clipboard is unavailable or rejects the write.
pub fn copy_guid_to_clipboard(guid: &str) -> Result<()> {
    copy_to_clipboard(guid).context("Error copying to clipboard")
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_cli::broken_pipe::BrokenPipe;
    use common_cli::test_writers::{ClosedPipe, FailingDisk};
    use uuid::Uuid;

    #[test]
    fn create_guid_empty_returns_all_zeros() {
        assert_eq!(create_guid(true), "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn create_guid_returns_a_valid_uuid() {
        let guid = create_guid(false);

        assert!(Uuid::parse_str(&guid).is_ok());
    }

    #[test]
    fn create_guid_returns_distinct_values() {
        assert_ne!(create_guid(false), create_guid(false));
    }

    #[test]
    fn generate_multiple_guid_writes_one_guid_per_line() {
        let mut output = Vec::new();

        generate_multiple_guid(3, &mut output).unwrap();

        let text = String::from_utf8(output).unwrap();
        assert!(text.ends_with('\n'));

        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        for line in lines {
            assert!(Uuid::parse_str(line).is_ok());
        }
    }

    #[test]
    fn generate_multiple_guid_maps_closed_pipe_to_the_marker() {
        let err = generate_multiple_guid(1, &mut ClosedPipe).unwrap_err();

        assert!(err.is::<BrokenPipe>());
    }

    #[test]
    fn generate_multiple_guid_keeps_other_write_errors_ordinary() {
        let err = generate_multiple_guid(1, &mut FailingDisk).unwrap_err();

        assert!(!err.is::<BrokenPipe>());
    }
}
