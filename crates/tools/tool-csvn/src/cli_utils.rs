use crate::models::CsvNConfig;
use anyhow::{anyhow, Result};
use clap::{Parser};
use std::collections::HashMap;
use std::path::PathBuf;
use common_cli::common_tool_args::CommonToolArgs;
use common_utils::constants::{CONFIG_UL_ITEM_LEVEL_2};
use common_utils::file_system::get_current_dir;

/// Normalizes a CSV file
///
/// Creates a normalized version of a CSV file, with missing fields filled by default values.
#[derive(Parser, Debug)]
#[command(about, long_about, version)]
pub struct CliArgs {

    /// Path to the input file.
    #[arg(short='f', long="file", required=true)]
    pub file: PathBuf,

    /// Headers of the CSV file, separated by a comma. Optional: If not informed, will try to infer from the first row of the file.
    #[arg(short='e', long="headers")]
    pub headers: Option<String>,

    /// Feedback interval, in rows. Will update progress on the console every X rows.
    #[arg(short='i', long="feedback-interval", default_value_t=100)]
    pub feedback_interval: usize,

    /// If set, will clean the rows from non-printable/utf-8 characters. Warning: This slows down the process by a lot!
    #[arg(short='c', long="clean-string", default_value_t=false)]
    pub clean_string: bool,

    /// Key=Value pairs to be used as default values for missing fields. To add multiple parameters, use this flag multiple times. If you want a single value for all missing fields, use * as the key, and inform the value.
    #[arg(short='v', long="value-map", required=true)]
    pub value_map: Vec<String>,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Displays runtime configuration information.
///
/// Shows input file, headers, cleaning options, and default mappings.
pub fn print_runtime_info(args: &CsvNConfig) {
    println!("{} Input file: {}", CONFIG_UL_ITEM_LEVEL_2, args.input_file.display());

    if args.headers.is_some() {
        println!("{} Headers: {:?}", CONFIG_UL_ITEM_LEVEL_2, args.headers);
    } else {
        println!("{} Headers: Will be inferred from file.", CONFIG_UL_ITEM_LEVEL_2);
    }

    println!("{} Clean string: {}", CONFIG_UL_ITEM_LEVEL_2, args.clean_string);
    println!("{} Default value map: {:?}", CONFIG_UL_ITEM_LEVEL_2, args.default_value_map);
    println!("{} Feedback Interval: {}", CONFIG_UL_ITEM_LEVEL_2, args.feedback_interval);

    println!(
        "{} Note: For performance reasons, malformed CSV lines will be skipped and not logged.",
        CONFIG_UL_ITEM_LEVEL_2
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
pub fn initialize() -> Result<CsvNConfig> {
    let args = CliArgs::parse();

    let current_working_dir = get_current_dir();

    let input_file = if !&args.file.is_absolute() {
        current_working_dir.join(&args.file)
    } else {
        (&args.file).clone()
    };

    if !input_file.exists() {
        return Err(anyhow!(
            "Input file does not exist. Please provide a valid path."
        ));
    }

    let headers: Option<Vec<String>> = match args.headers {
        Some(headers_arg) => Some(
            headers_arg
                .split(',')
                .map(|x| x.trim().to_string())
                .collect(),
        ),
        None => None,
    };

    let clean_string = args.clean_string;

    if args.value_map.len() == 0 {
        return Err(anyhow!(
            "Default value map is required. Please provide a valid key=value pair."
        ));
    }

    let default_value_map: HashMap<String, String> = args
        .value_map
        .iter()
        .map(|raw_value_pair| {
            let mut parts = raw_value_pair.splitn(2, '=');

            let key = parts
                .next()
                .unwrap_or("")
                .trim()
                .to_lowercase();

            let value = parts
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches(['"', '\''])
                .to_string();

            (key, value)
        })
        .collect();

    let feedback_interval = args.feedback_interval;

    let config = CsvNConfig::new(
        input_file,
        headers,
        clean_string,
        default_value_map,
        feedback_interval,
    );

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| { print_runtime_info(&config); }));

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_runtime_info_covers_header_and_clean_string_modes() {
        let with_headers = CsvNConfig::new(
            PathBuf::from("data.csv"),
            Some(vec!["a".to_string(), "b".to_string()]),
            true,
            HashMap::from([("*".to_string(), "x".to_string())]),
            100,
        );
        print_runtime_info(&with_headers);

        let inferred = CsvNConfig::new(PathBuf::from("data.csv"), None, false, HashMap::new(), 50);
        print_runtime_info(&inferred);
    }
}