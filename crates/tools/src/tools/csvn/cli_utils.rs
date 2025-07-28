use crate::tools::csvn::models::CsvNConfig;
use anyhow::{anyhow, Context, Result};
use clap::{Arg, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::{CSVN_APP_NAME, DASH_LINE};
use shared::constants::versions::CSVN_VERSION;
use shared::system::get_current_working_dir::get_current_working_dir;
use std::collections::HashMap;
use std::path::PathBuf;

/// Displays runtime configuration information.
///
/// Shows input file, headers, cleaning options, and default mappings.
pub fn print_runtime_info(args: &CsvNConfig) {
    println!("🚀 CSV Normalizer v{}", CSVN_VERSION);
    println!("{}", DASH_LINE);

    println!("📃 Input file: {}", args.input_file.display());

    if args.headers.is_some() {
        println!("🗣  Headers: {:?}", args.headers);
    } else {
        println!("🗣  Headers: Will be inferred from file.");
    }

    println!("🧼 Clean string: {}", args.clean_string);

    println!("🗺  Default value map: {:?}", args.default_value_map);

    println!("🔁 Feedback Interval: {}", args.feedback_interval);

    println!(
        "💡 Note: For performance reasons, malformed CSV lines will be skipped and not logged."
    );

    if args.clean_string {
        println!("\n⚠ Warning: This will slow down the process by a lot!\n");
    }

    println!();
}

/// Parses command-line arguments into CSV processing configuration.
///
/// Required: input file path and value-map pairs.
/// Optional: headers, feedback interval, string cleaning.
///
/// # Errors
/// Returns error if required arguments are missing or parsing fails
pub fn get_cli_arguments() -> Result<CsvNConfig> {
    let matches = Command::new(CSVN_APP_NAME)
        .add_basic_metadata(
            CSVN_VERSION,
            "CSV Normalizer",
            "Creates a normalized version of a CSV file, with missing fields filled by default values.")
        .arg(Arg::new("file")
            .long("file")
            .short('f')
            .required(true)
            .help("Path to the input file."))
        .arg(Arg::new("headers")
            .long("headers")
            .short('e')
            .help("Headers of the CSV file, separated by a comma. Optional: If not informed, will try to infer from the first row of the file."))
        .arg(Arg::new("feedback-interval")
            .long("feedback-interval")
            .short('i')
            .default_value("100")
            .help("Feedback interval, in rows. Will update progress on the console every X rows."))
        .arg(Arg::new("clean-string")
            .long("clean-string")
            .short('c')
            .action(clap::ArgAction::SetTrue)
            .help("If set, will clean the rows from non-printable/utf-8 characters. Warning: This slows down the process by a lot!"))
        .arg(Arg::new("value-map")
            .long("value-map")
            .short('v')
            .action(clap::ArgAction::Append)
            .required(true)
            .help("Key=Value pairs to be used as default values for missing fields. To add multiple parameters, use this flag multiple times. If you want a single value for all missing fields, use * as the key, and inform the value.")
        )
        .get_matches();

    let current_working_dir = get_current_working_dir();

    let input_file = if let Some(input_file_arg) = matches.get_one::<String>("file") {
        let input_file_path = PathBuf::from(input_file_arg);
        if !input_file_path.is_absolute() {
            current_working_dir.join(input_file_path)
        } else {
            input_file_path
        }
    } else {
        return Err(anyhow!(
            "Input file is required. Please provide a valid path."
        ));
    };

    let headers: Option<Vec<String>> = matches.get_one::<String>("headers").map(|headers_arg| {
        headers_arg
            .split(',')
            .map(|x| x.trim().to_string())
            .collect()
    });

    let clean_string = matches
        .get_one::<bool>("clean-string")
        .unwrap_or(&false)
        .clone();

    let default_value_map: HashMap<String, String> = matches
        .get_many::<String>("value-map")
        .context("Default value map is required. Please provide a valid key=value pair.")?
        .map(|raw_value_pair| {
            let mut parts = raw_value_pair.splitn(2, '=');
            let key = parts.next().unwrap_or("").trim().to_lowercase().to_string();
            let value = parts
                .next()
                .unwrap_or("")
                .trim_start_matches("\"")
                .trim_start_matches("'")
                .trim_end_matches("\"")
                .trim_end_matches("'")
                .trim()
                .to_string();
            (key, value)
        })
        .collect();

    let feedback_interval =
        if let Some(feedback_interval_arg) = matches.get_one::<String>("feedback-interval") {
            feedback_interval_arg.parse::<u64>().unwrap_or(100)
        } else {
            100
        };

    Ok(CsvNConfig::new(
        input_file,
        headers,
        clean_string,
        default_value_map,
        feedback_interval,
    ))
}
