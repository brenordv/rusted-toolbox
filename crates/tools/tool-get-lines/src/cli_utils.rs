use crate::models::GetLinesConfig;
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::tool_exit_helpers::exit_error;
use common_utils::constants::CONFIG_UL_ITEM_LEVEL_2;
use std::collections::HashSet;
use std::path::PathBuf;

/// Extracts lines from a text file.
///
/// Searches for one or more terms within a file and writes the matching lines, in input order,
/// either to the console or to a separate file per term.
#[derive(Parser, Debug)]
#[command(about, long_about, version)]
pub struct CliArgs {
    /// Comma-separated list of texts to search for (case-insensitive)
    #[arg(short = 's', long = "search", required = true)]
    search: String,

    /// Path to the input file
    #[arg(short = 'f', long = "file", required = true)]
    file: PathBuf,

    /// Output folder name. If not specified, results will be written to the console.
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    /// If set, line numbers will not be displayed in the output.
    #[arg(short = 'i', long = "hide-line-numbers")]
    hide_line_numbers: bool,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Splits, trims, lowercases, and deduplicates the raw `--search` value.
///
/// Terms are lowercased with Unicode-aware folding (the canonical form used for output
/// filenames), empty terms are dropped, and duplicates are removed while the first
/// occurrence of each term keeps its position.
pub fn normalize_search_terms(raw: &str) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();

    raw.split(',')
        .map(|term| term.trim().to_lowercase())
        .filter(|term| !term.is_empty())
        .filter(|term| seen.insert(term.clone()))
        .collect()
}

/// Displays runtime configuration information.
///
/// Shows the input file, the output destination, and the search terms.
pub fn print_runtime_info(args: &GetLinesConfig) {
    println!("{} Input File: {:?}", CONFIG_UL_ITEM_LEVEL_2, args.file);

    if let Some(output_folder) = args.output.as_deref() {
        println!(
            "{} Output Folder: {:?}",
            CONFIG_UL_ITEM_LEVEL_2, output_folder
        );
    } else {
        println!("{} Output: Console", CONFIG_UL_ITEM_LEVEL_2);
    };

    println!("{} Search: {:?}", CONFIG_UL_ITEM_LEVEL_2, args.search);

    println!();
}

/// Parses command-line arguments into application configuration.
///
/// # Arguments
/// - `--search, -s`: Required comma-separated list of search terms (case-insensitive)
/// - `--file, -f`: Required path to input text file
/// - `--output, -o`: Optional output folder (defaults to console output)
/// - `--hide-line-numbers, -i`: Optional flag to omit line numbers from output
///
/// # Behavior
/// - Normalizes search terms (trim, lowercase, drop empties, dedup preserving order)
/// - Exits with an error when no valid search term remains
pub fn initialize() -> GetLinesConfig {
    let args = CliArgs::parse();

    let search_terms = normalize_search_terms(&args.search);

    if search_terms.is_empty() {
        eprintln!("Error: No valid search terms provided.");
        exit_error();
    }

    let config = GetLinesConfig {
        search: search_terms,
        file: args.file,
        output: args.output,
        hide_line_numbers: args.hide_line_numbers,
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

    #[test]
    fn normalize_trims_and_lowercases() {
        assert_eq!(
            normalize_search_terms("  Error , WARNING "),
            vec!["error".to_string(), "warning".to_string()]
        );
    }

    #[test]
    fn normalize_drops_empty_terms() {
        assert_eq!(
            normalize_search_terms("error,,  ,warning"),
            vec!["error".to_string(), "warning".to_string()]
        );
    }

    #[test]
    fn normalize_dedups_preserving_first_seen_order() {
        assert_eq!(
            normalize_search_terms("beta,alpha,beta,ALPHA,gamma"),
            vec!["beta".to_string(), "alpha".to_string(), "gamma".to_string()]
        );
    }

    #[test]
    fn normalize_empty_input_yields_no_terms() {
        assert!(normalize_search_terms("  , , ").is_empty());
    }

    #[test]
    fn print_runtime_info_covers_console_and_file_output() {
        let console = GetLinesConfig {
            search: vec!["needle".to_string()],
            file: PathBuf::from("in.txt"),
            output: None,
            hide_line_numbers: false,
        };
        print_runtime_info(&console);

        let mut files = console.clone();
        files.output = Some(PathBuf::from("out"));
        print_runtime_info(&files);
    }
}
