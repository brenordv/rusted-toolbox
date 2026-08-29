use crate::models::GuidConfig;
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::tool_exit_helpers::exit_error;
use common_utils::constants::CONFIG_UL_ITEM_LEVEL_2;

/// Generates GUIDs (uuid-v4) values, including empty.
///
/// This tool can generate a single valid guid or an empty guid and copy this to the clipboard.
/// Alternatively, it can continuously generate valid guids and output them to the terminal.
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

/// Displays runtime configuration information.
///
/// Shows version, silence mode, interval settings, clipboard options, and empty GUID flags.
pub fn print_runtime_info(args: &GuidConfig) {
    if let Some(generate_target) = args.generate_multiple {
        println!(
            "{} Generate multiple guids: {}",
            CONFIG_UL_ITEM_LEVEL_2, generate_target
        );
        return;
    }

    println!(
        "{} Copy to clipboard: {}",
        CONFIG_UL_ITEM_LEVEL_2, args.add_to_clipboard
    );
    println!(
        "{} Empty Guid: {}",
        CONFIG_UL_ITEM_LEVEL_2, args.generate_empty_guid
    );
}

/// Parses command-line arguments into GUID generation configuration.
///
/// Supports single/continuous generation, clipboard copying, empty GUIDs, and silent mode.
///
/// # Errors
/// Terminates program if invalid arguments are provided
pub fn initialize() -> GuidConfig {
    let args = CliArgs::parse();

    let multiple_generation = match args.generate_multiple {
        Some(n) => {
            if n == 0 {
                eprintln!("Error: Invalid multiple generation count. Must be a positive integer.");
                exit_error();
            }

            Some(n)
        }
        None => None,
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
