use crate::models::{InputSource, OutputTarget, RemoveZwArgs};
use anyhow::{anyhow, Result};
use clap::{builder::NonEmptyStringValueParser, Arg, ArgAction, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;
use std::fs;
use std::path::PathBuf;

pub fn print_runtime_info(args: &RemoveZwArgs) {
    println!("Remove-ZW v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);

    println!("- Inputs:");
    for input in &args.inputs {
        match input {
            InputSource::Stdin => println!("  - stdin"),
            InputSource::File(path) => println!("  - {}", path.display()),
            InputSource::Directory(path) => println!("  - {} (dir)", path.display()),
        }
    }

    println!("- Output:");
    if args.in_place {
        println!("  - In place");
    } else if let Some(output) = &args.output {
        match output {
            OutputTarget::Stdout => println!("  - Stdout"),
            OutputTarget::File(path) => println!("  - {}", path.display()),
        }
    } else if args.inputs.iter().all(|input| matches!(input, InputSource::Stdin)) {
        println!("  - Stdout");
    } else {
        println!("  - Per-file cleaned output");
    }

    println!("- Verbose: {}", args.verbose);
    println!("- Recursive: {}", args.recursive);
    if args.extensions.is_empty() {
        println!("- Extensions: (all)");
    } else {
        println!("- Extensions: {:?}", args.extensions);
    }
    println!();
}

pub fn get_cli_arguments() -> RemoveZwArgs {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            "Remove zero-width Unicode format characters from text.",
            "Removes all Unicode format (Cf) characters from input text. With no FILE, or when FILE is -, read standard input.",
        )
        .preset_arg_verbose(None)
        .arg(
            Arg::new("no-header")
                .long("no-header")
                .short('n')
                .action(ArgAction::SetTrue)
                .help("Do not print header."),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .short('o')
                .value_name("FILE")
                .value_parser(NonEmptyStringValueParser::new())
                .help("Write output to FILE instead of stdout (use '-' for stdout)"),
        )
        .arg(
            Arg::new("in-place")
                .long("in-place")
                .action(ArgAction::SetTrue)
                .help("Overwrite input files in place (ignored for stdin)"),
        )
        .arg(
            Arg::new("recursive")
                .long("recursive")
                .short('r')
                .action(ArgAction::SetTrue)
                .help("When a directory is provided, process files recursively"),
        )
        .arg(
            Arg::new("extensions")
                .long("extensions")
                .short('e')
                .value_name("EXTS")
                .value_parser(NonEmptyStringValueParser::new())
                .help("Comma-separated list of file extensions to include (e.g. txt,md,rs)"),
        )
        .arg(
            Arg::new("files")
                .help("Files to process (reads from stdin if none or '-')")
                .action(ArgAction::Append)
                .num_args(0..),
        )
        .get_matches();

    let inputs = matches
        .get_many::<String>("files")
        .unwrap_or_default()
        .map(|value| map_input_value(value))
        .collect::<Vec<_>>();

    let inputs = if inputs.is_empty() {
        vec![InputSource::Stdin]
    } else {
        inputs
    };

    let output = matches
        .get_one::<String>("output")
        .map(|value| value.as_str())
        .map(parse_output_target);

    let extensions = matches
        .get_one::<String>("extensions")
        .map(|value| parse_extensions(value))
        .unwrap_or_default();

    RemoveZwArgs {
        inputs,
        output,
        in_place: matches.get_flag("in-place"),
        recursive: matches.get_flag("recursive"),
        extensions,
        verbose: matches.get_flag("verbose"),
        no_header: matches.get_flag("no-header"),
    }
}

pub fn validate_args(args: &RemoveZwArgs) -> Result<()> {
    let stdin_count = args
        .inputs
        .iter()
        .filter(|input| matches!(input, InputSource::Stdin))
        .count();

    if stdin_count > 1 {
        return Err(anyhow!("stdin can only be specified once"));
    }

    let has_file_inputs = args
        .inputs
        .iter()
        .any(|input| matches!(input, InputSource::File(_)));

    if args.in_place && !has_file_inputs {
        return Err(anyhow!("--in-place requires at least one file input"));
    }

    if args.in_place && args.output.is_some() {
        return Err(anyhow!("--in-place cannot be combined with --output"));
    }

    for input in &args.inputs {
        if let InputSource::File(path) = input {
            let metadata = fs::metadata(path)
                .map_err(|_| anyhow!("Input file does not exist: {}", path.display()))?;
            if !metadata.is_file() {
                return Err(anyhow!("Input path is not a file: {}", path.display()));
            }
        }
        if let InputSource::Directory(path) = input {
            let metadata = fs::metadata(path)
                .map_err(|_| anyhow!("Input directory does not exist: {}", path.display()))?;
            if !metadata.is_dir() {
                return Err(anyhow!("Input path is not a directory: {}", path.display()));
            }
        }
    }

    if let Some(OutputTarget::File(_)) = args.output {
        if args.inputs.len() > 1 || args.inputs.iter().any(|i| matches!(i, InputSource::Directory(_)))
        {
            return Err(anyhow!(
                "--output FILE requires a single file input (use '-' for stdout)"
            ));
        }
    }

    Ok(())
}

fn parse_output_target(value: &str) -> OutputTarget {
    if value == "-" {
        OutputTarget::Stdout
    } else {
        OutputTarget::File(value.into())
    }
}

fn map_input_value(value: &str) -> InputSource {
    if value == "-" {
        return InputSource::Stdin;
    }

    let path = PathBuf::from(value);

    match fs::metadata(&path) {
        Ok(metadata) => {
            if metadata.is_dir() {
                InputSource::Directory(path)
            } else {
                InputSource::File(path)
            }
        }
        Err(_) => InputSource::File(path),
    }
}

fn parse_extensions(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|ext| ext.trim())
        .filter(|ext| !ext.is_empty())
        .map(|ext| ext.trim_start_matches('.').to_lowercase())
        .collect()
}
