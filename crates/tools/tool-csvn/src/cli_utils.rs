use crate::models::CsvNConfig;
use anyhow::{anyhow, Result};
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::header_format::format_config_item;
use common_utils::file_system::get_current_dir;
use std::collections::HashMap;
use std::path::PathBuf;

/// Normalizes a CSV file
///
/// Creates a normalized version of a CSV file, with missing fields filled by default values.
#[derive(Parser, Debug)]
#[command(about, long_about, version)]
pub struct CliArgs {
    /// Path to the input file.
    #[arg(short = 'f', long = "file", required = true)]
    pub file: PathBuf,

    /// Headers of the CSV file, separated by a comma. Optional: If not informed, will try to infer from the first row of the file.
    #[arg(short = 'e', long = "headers")]
    pub headers: Option<String>,

    /// Feedback interval, in rows. Will update progress on the console every X rows.
    #[arg(short='i', long="feedback-interval", default_value_t=100, value_parser=clap::builder::RangedU64ValueParser::<usize>::new().range(1..))]
    pub feedback_interval: usize,

    /// If set, will clean the rows from non-printable/utf-8 characters. Warning: This slows down the process by a lot!
    #[arg(short = 'c', long = "clean-string", default_value_t = false)]
    pub clean_string: bool,

    /// Key=Value pairs to be used as default values for missing fields. To add multiple parameters, use this flag multiple times. If you want a single value for all missing fields, use * as the key, and inform the value.
    #[arg(short = 'v', long = "value-map", required = true)]
    pub value_map: Vec<String>,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Displays runtime configuration information.
///
/// Shows input file, headers, cleaning options, and default mappings.
pub fn print_runtime_info(args: &CsvNConfig) {
    println!(
        "{}",
        format_config_item("Input file", args.input_file.display())
    );

    if args.headers.is_some() {
        println!(
            "{}",
            format_config_item("Headers", format!("{:?}", args.headers))
        );
    } else {
        println!(
            "{}",
            format_config_item("Headers", "Will be inferred from file.")
        );
    }

    println!("{}", format_config_item("Clean string", args.clean_string));
    println!(
        "{}",
        format_config_item("Default value map", format!("{:?}", args.default_value_map))
    );
    println!(
        "{}",
        format_config_item("Feedback Interval", args.feedback_interval)
    );

    println!(
        "{}",
        format_config_item(
            "Note",
            "rows with a mismatched column count are repaired to fit the header; unparseable rows are skipped. Both counts are reported at the end."
        )
    );

    if args.clean_string {
        println!("\nWarning: This will slow down the process by a lot!\n");
    }

    println!();
}

/// Splits a comma-separated header string into trimmed header names.
///
/// Empty segments are kept as empty strings, so `"a,,b"` yields three headers, 
/// and an empty input yields a single empty header.
pub fn parse_headers(headers_arg: &str) -> Vec<String> {
    headers_arg
        .split(',')
        .map(|field| field.trim().to_string())
        .collect()
}

/// Builds the default-value map from raw `key=value` CLI pairs.
///
/// Keys are trimmed and lowercased (`*` is the wildcard key); values are
/// trimmed and stripped of surrounding single or double quotes. A pair without
/// `=` maps its key to an empty string. Duplicate keys keep the last value.
pub fn parse_value_map(raw_pairs: &[String]) -> HashMap<String, String> {
    raw_pairs
        .iter()
        .map(|raw_value_pair| {
            let mut parts = raw_value_pair.splitn(2, '=');

            let key = parts.next().unwrap_or("").trim().to_lowercase();

            let value = parts
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches(['"', '\''])
                .to_string();

            (key, value)
        })
        .collect()
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

    let input_file = if args.file.is_absolute() {
        args.file.clone()
    } else {
        current_working_dir.join(&args.file)
    };

    if !input_file.exists() {
        return Err(anyhow!(
            "Input file does not exist. Please provide a valid path."
        ));
    }

    let headers: Option<Vec<String>> = args.headers.map(|headers_arg| parse_headers(&headers_arg));

    let clean_string = args.clean_string;

    let default_value_map = parse_value_map(&args.value_map);

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
        Some(|| {
            print_runtime_info(&config);
        }),
    );

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::wildcard_key(&["*=N/A"], "*", "N/A")]
    #[case::key_is_lowercased(&["Name=John"], "name", "John")]
    #[case::double_quotes_trimmed(&["k=\"v\""], "k", "v")]
    #[case::single_quotes_trimmed(&["k='v'"], "k", "v")]
    #[case::bare_key_maps_to_empty(&["flag"], "flag", "")]
    fn parse_value_map_parses_single_pair(
        #[case] raw: &[&str],
        #[case] key: &str,
        #[case] value: &str,
    ) {
        let raw: Vec<String> = raw.iter().map(|pair| pair.to_string()).collect();

        let map = parse_value_map(&raw);

        assert_eq!(map.len(), 1);
        assert_eq!(map.get(key).map(String::as_str), Some(value));
    }

    #[test]
    fn parse_value_map_duplicate_key_keeps_last_value() {
        let raw = vec!["city=Paris".to_string(), "City=Rome".to_string()];

        let map = parse_value_map(&raw);

        assert_eq!(map.len(), 1);
        assert_eq!(map.get("city").map(String::as_str), Some("Rome"));
    }

    #[rstest]
    #[case::split_and_trim("name, city ,age", &["name", "city", "age"])]
    #[case::empty_segments_kept("a,,b", &["a", "", "b"])]
    #[case::empty_input_yields_one_empty_header("", &[""])]
    fn parse_headers_splits_on_commas(#[case] raw: &str, #[case] expected: &[&str]) {
        assert_eq!(parse_headers(raw), expected);
    }

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
