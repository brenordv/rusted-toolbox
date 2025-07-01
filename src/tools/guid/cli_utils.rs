use crate::shared::command_line::cli_builder::CommandExt;
use crate::shared::constants::general::{DASH_LINE, GUID_APP_NAME};
use crate::shared::constants::versions::GUID_VERSION;
use crate::tools::guid::models::GuidArgs;
use clap::{Arg, Command};

/// Displays runtime configuration information.
///
/// Shows version, silence mode, interval settings, clipboard options, and empty GUID flags.
pub fn print_runtime_info(args: &GuidArgs) {
    println!("🚀 Guid v{}", GUID_VERSION);
    println!("{}", DASH_LINE);
    println!("🔇 Silence: {}", args.silent);

    if let Some(interval) = args.generate_on_interval {
        println!("⏰  Generate guid every (s): {}", interval);
    } else {
        println!("📋 Copy to clipboard: {}", args.add_to_clipboard);
        println!("📦 Empty Guid: {}", args.generate_empty_guid);
    }
}

/// Parses command-line arguments into GUID generation configuration.
///
/// Supports single/continuous generation, clipboard copying, empty GUIDs, and silent mode.
///
/// # Errors
/// Terminates program if invalid arguments are provided
pub fn get_cli_arguments() -> GuidArgs {
    let matches = Command::new(GUID_APP_NAME)
        .add_basic_metadata(
            GUID_VERSION,
            "Generates GUIDs (uuid-v4) values, including empty.",
            "This tool can generate a single valid guid or an empty guid and copy this to the clipboard. Alternatively, it can continuously generate valid guids and output them to the terminal.")
        .arg(Arg::new("copy-to-clipboard")
            .long("copy-to-clipboard")
            .short('c')
            .action(clap::ArgAction::SetTrue)
            .help("If set, will copy the generated guid to the clipboard. Does not work with continuous generation. (Default: false)"))
        .arg(Arg::new("empty")
            .long("empty")
            .short('e')
            .action(clap::ArgAction::SetTrue)
            .help("If set, will generate an empty guid. Does not work in conjunction with continuous generation. (Default: false)"))
        .arg(Arg::new("silent")
            .long("silent")
            .short('s')
            .action(clap::ArgAction::SetTrue)
            .help("If set, will only print the guid values. (Default: false)"))
        .arg(Arg::new("continuous-generation")
            .long("continuous-generation")
            .short('i')
            .value_parser(clap::value_parser!(f64))
            .help("If set with positive value, will continuously generate guids, and print them to the terminal. (Default: -)"))
        .get_matches();

    GuidArgs {
        add_to_clipboard: matches.get_flag("copy-to-clipboard"),
        generate_empty_guid: matches.get_flag("empty"),
        silent: matches.get_flag("silent"),
        generate_on_interval: match matches.get_one::<f64>("continuous-generation") {
            None => None,
            Some(interval) => Some(interval.clone()),
        },
    }
}

/// Validates command-line arguments for compatibility and correctness.
///
/// Ensures continuous generation doesn't conflict with clipboard/empty options and interval is positive.
pub fn validate_cli_arguments(args: &GuidArgs) {
    if args.generate_on_interval.is_some() && (args.generate_empty_guid || args.add_to_clipboard) {
        eprintln!("Continuous generation cannot be used with empty guids or copying to clipboard.");
        std::process::exit(1);
    }

    if let Some(interval) = args.generate_on_interval {
        if interval > 0.0 {
            return;
        }
        eprintln!("Interval must be greater than 0.");
        std::process::exit(1);
    }
}
