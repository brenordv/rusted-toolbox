use clap::{Arg, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::{DASH_LINE};
use shared::system::tool_exit_helpers::exit_error;
use std::path::Path;
use crate::models::GetLinesArgs;

/// Displays runtime configuration information.
///
/// Shows version, input file, output destination, worker count, and search terms.
pub fn print_runtime_info(args: &GetLinesArgs) {
    println!("🚀 Get-Lines v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);
    println!("📥 Input File: {}", args.file);

    if let Some(output_folder) = args.output.as_deref() {
        println!("📤 Output Folder: {}", output_folder);
    } else {
        println!("📺 Output: Console");
    };

    println!("🧠 Worker Count: {}", args.workers);
    println!("🔍‍ Search: {:?}", args.search);

    if args.workers > 1 {
        println!(
            "⚠ Warning: Output will not be in the same order as the input due to parallel processing."
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
pub fn get_cli_arguments() -> GetLinesArgs {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            "Extracts lines from a text file.",
            "Searches for specific text within a file and outputs the lines containing the text. Supports parallel processing for faster search.")
        .preset_arg_verbose(None)
        .arg(Arg::new("search")
            .long("search")
            .short('s')
            .help("Comma-separated list of texts to search for (case-insensitive)")
            .required(true))
        .arg(Arg::new("file")
            .long("file")
            .short('f')
            .help("Path to the input file")
            .required(true))
        .arg(Arg::new("output")
            .long("output")
            .short('o')
            .help("Output folder name. If not specified, results will be written to the console."))
        .arg(Arg::new("workers")
            .long("workers")
            .short('w')
            .help("Number of workers for parallel processing")
            .default_value("1"))
        .arg(Arg::new("hide-line-numbers")
                 .long("hide-line-numbers")
                 .short('i')
                 .action(clap::ArgAction::SetTrue)
                 .help("If set, line numbers will not be displayed in the output. (Default: false)"),
        )
        .arg(Arg::new("hide-runtime-info")
                 .long("hide-runtime-info")
                 .short('d')
                 .action(clap::ArgAction::SetTrue)
                 .help("If set, will not print the the runtime information at the beginning of the program. (Default: false)"),
        )
        .get_matches();

    let raw_terms: Vec<&str> = matches
        .get_one::<String>("search")
        .unwrap()
        .split(',')
        .collect();

    let search_terms: Vec<String> = raw_terms
        .iter()
        .map(|term| term.trim().to_lowercase())
        .filter(|term| !term.is_empty())
        .collect();

    GetLinesArgs {
        search: search_terms,
        file: matches.get_one::<String>("file").unwrap().clone(),
        output: matches.get_one::<String>("output").cloned(),
        workers: matches
            .get_one::<String>("workers")
            .unwrap()
            .parse()
            .unwrap_or(1),
        hide_line_numbers: matches.get_flag("hide-line-numbers"),
        hide_runtime_info: matches.get_flag("hide-runtime-info"),
    }
}

/// Validates parsed command-line arguments and displays warnings.
///
/// Ensures search terms are provided and the worker count is valid.
/// Warns about output ordering when using multiple workers.
///
/// # Arguments
/// - `args` - Parsed command-line arguments to validate
///
/// # Behavior
/// - Exits with code 1 if no valid search terms are provided, worker count <= zero, or if the input
/// file doesn't exit.

pub fn validate_cli_arguments(args: &GetLinesArgs) {
    if args.search.is_empty() {
        eprintln!("Error: No valid search terms provided.");
        exit_error();
    }

    if args.workers == 0 {
        eprintln!("Error: --workers must be greater than 0.");
        exit_error();
    }

    let input_file = Path::new(&args.file);

    if !input_file.exists() {
        eprintln!("Error: Input file does not exist: {}", args.file);
        exit_error();
    }
}