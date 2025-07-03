use crate::shared::command_line::cli_builder::CommandExt;
use crate::shared::constants::general::{DASH_LINE, TS_APP_NAME};
use crate::shared::constants::versions::TS_VERSION;
use clap::{Arg, Command};

use crate::tools::ts::models::TsArgs;

/// Displays runtime information for the timestamp converter.
///
/// Shows version, divider line, and input (or "(Current time)" if no input provided).
pub fn print_runtime_info(args: &TsArgs) {
    println!("🚀 Timestamp Converter v{}", TS_VERSION);
    println!("{}", DASH_LINE);

    let input = if args.input.is_empty() {
        "(Current time)"
    } else {
        &args.input
    };

    println!("🔢 Input: {}", input);

    println!();
}

/// Parses command-line arguments for timestamp conversion.
///
/// Accepts optional input that can be Unix timestamp, datetime string, or empty for current time.
/// Multiple input arguments are joined with spaces.
///
/// # Returns
/// TsArgs struct containing the parsed input string
pub fn get_cli_arguments() -> TsArgs {
    let matches = Command::new(TS_APP_NAME)
        .add_basic_metadata(
            TS_VERSION,
            "A simple utility to convert Unix timestamps to date time and vice versa.",
            "This tool receives a Unix timestamp and converts it to a date time (ISO8601) or vice versa.",
        ).arg(
        Arg::new("input")
            .value_name("input")
            .action(clap::ArgAction::Append)
            .required(false)
            .help("Input that will be processed. Only one input is valid, but no need to use quotes.")
    ).get_matches();

    let input: String = matches
        .get_many::<String>("input")
        .unwrap_or_default()
        .map(|s| s.clone())
        .collect::<Vec<String>>()
        .join(" ");

    TsArgs { input }
}
