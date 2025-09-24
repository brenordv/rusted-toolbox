use crate::models::SplitArgs;
use clap::{Arg, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;
use shared::system::get_current_working_dir::get_current_working_dir;
use std::path::{Path, PathBuf};

/// Displays runtime configuration for the file splitting operation.
///
/// Shows version, input file, output directory, lines per file, prefix, and CSV mode status.
pub fn print_runtime_info(args: &SplitArgs) {
    println!("File Splitter v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);

    println!("- Input file: {}", args.input_file);
    println!("- Output dir: {}", args.output_dir);
    println!("- Lines per file: {}", args.lines_per_file);
    println!("- File prefix: {}", args.prefix);
    println!("- Csv Mode: {}", args.csv_mode);

    println!();
}

/// Parses command-line arguments for file splitting configuration.
///
/// Creates SplitArgs with file paths, line count, prefix, CSV mode, and feedback settings.
/// Resolves relative paths to absolute paths using current working directory.
///
/// # Panics
/// Panics if required file argument is missing or numeric arguments cannot be parsed
pub fn get_cli_arguments() -> SplitArgs {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            "File splitter",
            "Split files by number of lines.")
        .arg(Arg::new("file")
            .long("file")
            .short('f')
            .required(true)
            .help("Path to the input file."))
        .arg(Arg::new("output-dir")
            .long("output-dir")
            .short('o')
            .help("Output directory. If not set, will use the same directory as the input file."))
        .arg(Arg::new("lines-per-file")
            .long("lines-per-file")
            .short('l')
            .default_value("100")
            .help("Number of lines per file."))
        .arg(Arg::new("file-prefix")
            .long("file-prefix")
            .short('p')
            .default_value("split")
            .help("Prefix for the output files."))
        .arg(Arg::new("feedback-interval")
            .long("feedback-interval")
            .short('i')
            .default_value("100")
            .help("Interval between feedback update in number of lines."))
        .arg(Arg::new("csv-mode")
            .long("csv-mode")
            .short('c')
            .action(clap::ArgAction::SetTrue)
            .help("If set, will use the first line of the input file as headers and propagate it to the output files. This will not count as the number of lines per file."))
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
        panic!("This should not happen, but it did. Please report this bug to the developers.");
    };

    // Process output directory
    let output_dir = if let Some(output_dir_arg) = matches.get_one::<String>("output-dir") {
        let output_dir_path = PathBuf::from(output_dir_arg);
        if !output_dir_path.is_absolute() {
            current_working_dir
                .join(output_dir_path)
                .to_string_lossy()
                .to_string()
        } else {
            output_dir_path.to_string_lossy().to_string()
        }
    } else {
        // Use the same directory as the input file
        input_file
            .parent()
            .unwrap_or(&current_working_dir)
            .to_string_lossy()
            .to_string()
    };

    // Process lines per file
    let lines_per_file = matches
        .get_one::<String>("lines-per-file")
        .unwrap()
        .parse::<usize>()
        .expect("Invalid number for lines-per-file");

    // Process file prefix
    let prefix = matches
        .get_one::<String>("file-prefix")
        .unwrap()
        .to_string();

    // Process CSV mode
    let csv_mode = matches.get_flag("csv-mode");

    // Extract filename without extension
    let input_filename_without_extension = input_file
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let feedback_interval =
        if let Some(feedback_interval_arg) = matches.get_one::<String>("feedback-interval") {
            feedback_interval_arg.parse::<usize>().unwrap_or(100)
        } else {
            100
        };

    // Return SplitArgs instance
    SplitArgs {
        input_file: input_file.to_string_lossy().to_string(),
        output_dir,
        input_filename_without_extension,
        lines_per_file,
        prefix,
        csv_mode,
        feedback_interval,
    }
}

/// Validates command-line arguments and creates output directory if needed.
///
/// Checks input file exists, lines per file is greater than zero, and ensures output directory exists.
/// Exits program with error code if validation fails.
pub fn ensure_cli_arguments_are_valid(args: &SplitArgs) {
    // Validate input file exists
    if !Path::new(&args.input_file).exists() {
        eprintln!("Error: Input file '{}' does not exist", args.input_file);
        std::process::exit(1);
    }

    // Validate lines_per_file is greater than 0
    if args.lines_per_file == 0 {
        eprintln!("Error: Lines per file must be greater than 0");
        std::process::exit(1);
    }

    // Set up the output directory
    let output_dir = PathBuf::from(args.output_dir.clone());

    // Create the output directory if it doesn't exist
    if !output_dir.exists() {
        std::fs::create_dir_all(&output_dir).expect("Failed to create output directory");
    }
}
