use anyhow::Result;
use common_cli::tool_exit_helpers::exit_error;
use common_utils_ext::copy_string_to_clipboard::copy_to_clipboard;
use common_utils_ext::new_guid::new_guid;
use tracing::error;

pub fn generate_multiple_guid(target_guid_count: usize) -> Result<()> {
    for _ in 0..target_guid_count {
        let guid = create_guid(false);
        print!("{}\r", guid);
    }

    Ok(())
}

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
