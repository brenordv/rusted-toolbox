use crate::models::GetLinesConfig;
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::tool_exit_helpers::exit_error;
use common_utils::constants::CONFIG_UL_ITEM_LEVEL_2;
use std::path::PathBuf;

/// Extracts lines from a text file.
///
/// Searches for specific text within a file and outputs the lines containing the text. Supports parallel processing for faster search.
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

    /// Number of workers for parallel processing
    #[arg(short = 'w', long = "workers", default_value_t = 1)]
    workers: usize,

    /// If set, line numbers will not be displayed in the output.
    #[arg(short = 'i', long = "hide-line-numbers")]
    hide_line_numbers: bool,

    /// If set, runtime information will not be printed at the beginning of the program.
    #[arg(short = 'd', long = "hide-runtime-info")]
    hide_runtime_info: bool,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Displays runtime configuration information.
///
/// Shows version, input file, output destination, worker count, and search terms.
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

    println!("{} Worker Count: {}", CONFIG_UL_ITEM_LEVEL_2, args.workers);
    println!("{} Search: {:?}", CONFIG_UL_ITEM_LEVEL_2, args.search);

    if args.workers > 1 {
        println!(
            "WARNING: Output will not be in the same order as the input due to parallel processing."
        );
    }

    println!();
}

/// Parses command-line arguments into application configuration.
///
/// Defines and processes CLI arguments for search terms, input file, output options,
/// and processing settings.
///
/// # Arguments
/// - `--search, -s`: Required comma-separated list of search terms (case-insensitive)
/// - `--file, -f`: Required path to input text file
/// - `--output, -o`: Optional output folder (defaults to console output)
/// - `--workers, -w`: Optional worker thread count (defaults to 1)
/// - `--hide-line-numbers, -i`: Optional flag to omit line numbers from output
/// - `--hide-runtime-info, -d`: Optional flag to suppress runtime information display
///
/// # Returns
/// `GetLinesArgs` struct containing parsed and processed configuration
///
/// # Behavior
/// - Trims and converts search terms to lowercase
/// - Filters out empty search terms
/// - Defaults workers to 1 if parsing fails
/// - Panics if required arguments are missing
pub fn initialize() -> GetLinesConfig {
    let args = CliArgs::parse();

    let raw_terms: Vec<&str> = args.search.split(',').collect();

    let search_terms: Vec<String> = raw_terms
        .iter()
        .map(|term| term.trim().to_lowercase())
        .filter(|term| !term.is_empty())
        .collect();

    if search_terms.is_empty() {
        eprintln!("Error: No valid search terms provided.");
        exit_error();
    }

    if args.workers <= 0 {
        eprintln!("Error: --workers must be greater than 0.");
        exit_error();
    }

    if !args.file.exists() {
        eprintln!("Error: Input file does not exist: {:?}", args.file);
        exit_error();
    }

    let config = GetLinesConfig {
        search: search_terms,
        file: args.file,
        output: args.output,
        workers: args.workers,
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
    use tempfile::tempdir;

    fn args_with(file: String) -> GetLinesConfig {
        GetLinesConfig {
            search: vec!["needle".to_string()],
            file: PathBuf::from(file),
            output: None,
            workers: 1,
            hide_line_numbers: false,
        }
    }

    #[test]
    fn print_runtime_info_covers_console_and_file_output() {
        let console = args_with("in.txt".to_string());
        print_runtime_info(&console);

        let mut files = args_with("in.txt".to_string());
        files.output = Some(PathBuf::from("out"));
        files.workers = 4;
        print_runtime_info(&files);
    }
}
