use crate::models::{InputSource, OutputTarget, RemoveZwArgs};
use anyhow::{anyhow, Result};
use clap::{builder::NonEmptyStringValueParser, Arg, ArgAction, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;
use std::fs;
use std::path::PathBuf;

/// Print runtime info to stderr, keeping stdout for cleaned content and the
/// dry-run report so both stay pipeable.
pub fn print_runtime_info(args: &RemoveZwArgs) {
    eprintln!("Remove-ZW v{}", env!("CARGO_PKG_VERSION"));
    eprintln!("{}", DASH_LINE);

    eprintln!("- Inputs:");
    for input in &args.inputs {
        match input {
            InputSource::Stdin => {
                eprintln!("  - stdin (filter mode: pipe input or pass a file/dir; Ctrl+Z then Enter to end)")
            }
            InputSource::File(path) => eprintln!("  - {}", path.display()),
            InputSource::Directory(path) => eprintln!("  - {} (dir)", path.display()),
        }
    }

    eprintln!("- Output:");
    if args.dry_run {
        eprintln!("  - Dry run (nothing written)");
    } else if args.in_place {
        eprintln!("  - In place");
    } else if let Some(output) = &args.output {
        match output {
            OutputTarget::Stdout => eprintln!("  - Stdout"),
            OutputTarget::File(path) => eprintln!("  - {}", path.display()),
        }
    } else if args
        .inputs
        .iter()
        .all(|input| matches!(input, InputSource::Stdin))
    {
        eprintln!("  - Stdout");
    } else {
        eprintln!("  - Per-file cleaned output");
    }

    eprintln!("- Verbose: {}", args.verbose);
    eprintln!("- Recursive: {}", args.recursive);
    if args.extensions.is_empty() {
        eprintln!("- Extensions: (all)");
    } else {
        eprintln!("- Extensions: {:?}", args.extensions);
    }
    eprintln!();
}

pub fn get_cli_arguments() -> RemoveZwArgs {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            "Remove zero-width Unicode format characters from text.",
            "Removes all Unicode format (Cf) characters from input text. With no FILE, or when FILE is -, it reads standard input and works as a filter (for example: cat file | remove-zw). To clean files on disk, pass a file or directory path.",
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
            Arg::new("dry-run")
                .long("dry-run")
                .short('d')
                .action(ArgAction::SetTrue)
                .help("Report which files would be modified and how, without writing anything"),
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
                .help("Files or directories to process; with none (or '-') reads stdin as a filter")
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
        dry_run: matches.get_flag("dry-run"),
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

    // --in-place needs no file-input guard: a directory expands to files, and
    // a stdin input is a documented no-op that still emits to stdout.
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
        if args.inputs.len() > 1
            || args
                .inputs
                .iter()
                .any(|i| matches!(i, InputSource::Directory(_)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn args_with(inputs: Vec<InputSource>) -> RemoveZwArgs {
        RemoveZwArgs {
            inputs,
            output: None,
            in_place: false,
            recursive: false,
            extensions: Vec::new(),
            verbose: false,
            no_header: true,
            dry_run: false,
        }
    }

    #[test]
    fn in_place_directory_is_allowed() {
        // B2: --in-place over a directory must validate.
        let dir = tempdir().unwrap();
        let mut args = args_with(vec![InputSource::Directory(dir.path().to_path_buf())]);
        args.in_place = true;
        assert!(validate_args(&args).is_ok());
    }

    #[test]
    fn in_place_stdin_only_is_allowed() {
        // B3: --in-place with only stdin is a no-op, not an error.
        let mut args = args_with(vec![InputSource::Stdin]);
        args.in_place = true;
        assert!(validate_args(&args).is_ok());
    }

    #[test]
    fn in_place_with_output_is_rejected() {
        let dir = tempdir().unwrap();
        let mut args = args_with(vec![InputSource::Directory(dir.path().to_path_buf())]);
        args.in_place = true;
        args.output = Some(OutputTarget::Stdout);
        assert!(validate_args(&args).is_err());
    }

    #[test]
    fn output_file_requires_single_file_input() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        fs::write(&a, b"a").unwrap();
        fs::write(&b, b"b").unwrap();

        let mut args = args_with(vec![InputSource::File(a), InputSource::File(b)]);
        args.output = Some(OutputTarget::File(dir.path().join("out.txt")));
        assert!(validate_args(&args).is_err());
    }

    #[test]
    fn stdin_specified_twice_is_rejected() {
        let args = args_with(vec![InputSource::Stdin, InputSource::Stdin]);
        assert!(validate_args(&args).is_err());
    }

    #[test]
    fn missing_input_file_is_rejected() {
        let dir = tempdir().unwrap();
        let args = args_with(vec![InputSource::File(dir.path().join("nope.txt"))]);
        assert!(validate_args(&args).is_err());
    }

    #[test]
    fn parse_extensions_normalizes_entries() {
        assert_eq!(
            parse_extensions(" .TXT, md ,,rs "),
            vec!["txt".to_string(), "md".to_string(), "rs".to_string()]
        );
        assert!(parse_extensions("  ,, ").is_empty());
    }

    #[test]
    fn print_runtime_info_covers_dry_run_and_default_modes() {
        // Smoke test: the header prints to stderr for both a dry-run and a
        // default per-file invocation without panicking.
        let mut dry = args_with(vec![InputSource::Directory(PathBuf::from("docs"))]);
        dry.dry_run = true;
        dry.recursive = true;
        dry.extensions = vec!["txt".to_string()];
        print_runtime_info(&dry);

        let default = args_with(vec![InputSource::File(PathBuf::from("a.txt"))]);
        print_runtime_info(&default);

        let to_stdout = args_with(vec![InputSource::Stdin]);
        print_runtime_info(&to_stdout);
    }
}
