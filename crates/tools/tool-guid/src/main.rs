use crate::cli_utils::initialize;
use crate::guid_app::{copy_guid_to_clipboard, create_guid, generate_multiple_guid};
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use tracing::error;

mod cli_utils;
mod guid_app;
mod models;

/// GUID generator tool.
///
/// Parses arguments, then generates a single guid (optionally empty, optionally
/// copied to the clipboard) or N guids printed one per line.
fn main() {
    let args = initialize();

    if let Some(target_guid_count) = args.generate_multiple {
        let mut stdout = std::io::stdout();
        if let Err(e) = generate_multiple_guid(target_guid_count, &mut stdout) {
            error!("Error while writing guids: {}", e);
            exit_error();
        }
    } else {
        let guid = create_guid(args.generate_empty_guid);

        println!("{}", guid);

        if args.add_to_clipboard {
            copy_guid_to_clipboard(guid);
        }
    }

    exit_success();
}
