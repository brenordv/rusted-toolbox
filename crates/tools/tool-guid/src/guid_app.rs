use common_cli::tool_exit_helpers::exit_error;
use common_utils_ext::copy_string_to_clipboard::copy_to_clipboard;
use common_utils_ext::new_guid::new_guid;
use std::io::Write;
use tracing::error;

/// Writes `target_guid_count` freshly generated guids to `output`, one per line.
pub fn generate_multiple_guid(
    target_guid_count: usize,
    output: &mut impl Write,
) -> std::io::Result<()> {
    for _ in 0..target_guid_count {
        writeln!(output, "{}", create_guid(false))?;
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

/// Copies GUID to the system clipboard.
///
/// Terminates the program with an error message if the clipboard operation fails.
pub fn copy_guid_to_clipboard(guid: String) {
    if let Err(e) = copy_to_clipboard(&guid) {
        error!("Error copying to clipboard: {}", e);
        exit_error();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
