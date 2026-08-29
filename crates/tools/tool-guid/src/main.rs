use crate::cli_utils::initialize;
use crate::guid_app::{copy_guid_to_clipboard, create_guid, generate_multiple_guid};
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

mod cli_utils;
mod guid_app;
mod models;

/// GUID generator tool.
///
/// Parses arguments, validates configuration, and generates GUIDs either once or continuously.
fn main() {
    let args = initialize();

    if let Some(target_guid_count) = args.generate_multiple {
        let _ = generate_multiple_guid(target_guid_count).inspect_err(|e| {
            error!("Error during continuous generation: {}", e);
            exit_error();
        });
    } else {
        let guid = create_guid(args.generate_empty_guid);

        print!("{}", guid);

        if args.add_to_clipboard {
            copy_guid_to_clipboard(guid);
        }
    }

    exit_success();
}
