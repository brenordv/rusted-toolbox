use crate::models::GuidConfig;
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::header_format::format_config_item;
use common_cli::tool_exit_helpers::exit_error;

/// Generates GUIDs (uuid-v4) values, including empty.
///
/// This tool can generate a single valid guid or an empty guid and copy this to the clipboard.
/// Alternatively, it can generate N guids and print them one per line.
#[derive(Parser, Debug)]
#[command(about, long_about, version)]
pub struct CliArgs {
    /// If set, will copy the generated guid to the clipboard. Does not work with generate_multiple.
    #[arg(
        short = 'c',
        long = "copy-to-clipboard",
        default_value_t = false,
        conflicts_with = "generate_multiple"
    )]
    pub copy_to_clipboard: bool,

    /// If set, will generate an empty guid. Does not work with generate_multiple.
    #[arg(
        short = 'e',
        long = "empty",
        default_value_t = false,
        conflicts_with = "generate_multiple"
    )]
    pub generate_empty_guid: bool,

    /// If set, will generate N guids, and print them to the terminal.
    #[arg(
        short='m',
        long="multiple",
        conflicts_with_all = ["generate_empty_guid", "copy_to_clipboard"]
    )]
    pub generate_multiple: Option<usize>,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Prints the tool section of the `--app-header` block: the multiple-generation
/// target when set, otherwise the clipboard and empty-guid flags.
pub fn print_runtime_info(args: &GuidConfig) {
    if let Some(generate_target) = args.generate_multiple {
        println!(
            "{}",
            format_config_item("Generate multiple guids", generate_target)
        );
        return;
    }

    println!(
        "{}",
        format_config_item("Copy to clipboard", args.add_to_clipboard)
    );
    println!(
        "{}",
        format_config_item("Empty Guid", args.generate_empty_guid)
    );
}

/// Rejects a zero generation count; `None` (single-guid mode) and any positive
/// count pass through unchanged.
fn validate_multiple(generate_multiple: Option<usize>) -> anyhow::Result<Option<usize>> {
    match generate_multiple {
        Some(0) => {
            anyhow::bail!("Invalid multiple generation count. Must be a positive integer.")
        }
        other => Ok(other),
    }
}

/// Parses command-line arguments into GUID generation configuration.
///
/// Validates the multiple-generation count, then boots logging and the optional
/// app header.
///
/// # Errors
/// Terminates program if invalid arguments are provided
pub fn initialize() -> GuidConfig {
    let args = CliArgs::parse();

    let multiple_generation = match validate_multiple(args.generate_multiple) {
        Ok(v) => v,
        Err(err) => {
            // Logging is not installed yet at this point, so report on stderr directly.
            eprintln!("Error: {}", err);
            exit_error();
        }
    };

    let config = GuidConfig {
        add_to_clipboard: args.copy_to_clipboard,
        generate_empty_guid: args.generate_empty_guid,
        generate_multiple: multiple_generation,
    };

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| {
            print_runtime_info(&config);
        }),
    );

    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_has_no_conflicting_flags() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn validate_multiple_rejects_zero() {
        assert!(validate_multiple(Some(0)).is_err());
    }

    #[test]
    fn validate_multiple_accepts_positive_count() {
        assert_eq!(validate_multiple(Some(3)).unwrap(), Some(3));
    }

    #[test]
    fn validate_multiple_accepts_single_guid_mode() {
        assert_eq!(validate_multiple(None).unwrap(), None);
    }
}
