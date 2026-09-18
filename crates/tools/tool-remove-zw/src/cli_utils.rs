use crate::models::{InputSource, OutputTarget, RemoveZwArgs};
use anyhow::{Result, anyhow};
use clap::{Parser, builder::NonEmptyStringValueParser};
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::header_format::{format_config_item, format_config_label};
use common_utils::constants::CONFIG_UL_ITEM_LEVEL_3;
use std::fs;
use std::path::PathBuf;

/// Remove zero-width Unicode format characters from text.
///
/// Removes all Unicode format (Cf) characters from input text. With no FILE, or when FILE is -,
/// it reads standard input and works as a filter (for example: cat file | remove-zw). To clean
/// files on disk, pass a file or directory path.
#[derive(Parser, Debug)]
#[command(about, long_about, version)]
struct CliArgs {
    /// Files or directories to process; with none (or '-') reads stdin as a filter
    #[arg(num_args = 0..)]
    pub files: Vec<String>,

    /// Write output to FILE instead of stdout (use '-' for stdout)
    #[arg(
        short = 'o',
        long = "output",
        value_name = "FILE",
        value_parser = NonEmptyStringValueParser::new()
    )]
    pub output: Option<String>,

    /// Overwrite input files in place (ignored for stdin)
    #[arg(long = "in-place")]
    pub in_place: bool,

    /// When a directory is provided, process files recursively
    #[arg(short = 'r', long = "recursive")]
    pub recursive: bool,

    /// Report which files would be modified and how, without writing anything
    #[arg(short = 'd', long = "dry-run")]
    pub dry_run: bool,

    /// Report like --dry-run and exit 0 (nothing to change), 1 (changes needed), or 2 (error)
    #[arg(long = "check", conflicts_with_all = ["in_place", "output", "dry_run"])]
    pub check: bool,

    /// With --check, exit 1 when any file was skipped (binary, UTF-16/32, extension filter)
    #[arg(long = "fail-on-skip", requires = "check")]
    pub fail_on_skip: bool,

    /// Keep a leading UTF-8 byte-order mark instead of stripping it
    #[arg(long = "keep-bom")]
    pub keep_bom: bool,

    /// Comma-separated list of file extensions to include (e.g. txt,md,rs)
    #[arg(
        short = 'e',
        long = "extensions",
        value_name = "EXTS",
        value_parser = NonEmptyStringValueParser::new()
    )]
    pub extensions: Option<String>,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Parses the command line and returns the runtime configuration together
/// with the shared tool flags. clap itself exits (code 2) on usage errors, so
/// this cannot fail; cross-flag validation is `main`'s next step, kept
/// separate so the exit code can depend on `--check`.
pub fn parse_and_build() -> (RemoveZwArgs, CommonToolArgs) {
    let args = CliArgs::parse();
    let config = build_args(&args);
    (config, args.common)
}

/// Boots logging and the optional `--app-header` block for a validated config.
pub fn boot(common: &CommonToolArgs, config: &RemoveZwArgs) {
    common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        true,
        false,
        Some(|| {
            print_header(config);
        }),
    );
}

/// Maps the parsed CLI arguments into the runtime configuration.
fn build_args(args: &CliArgs) -> RemoveZwArgs {
    let inputs = args
        .files
        .iter()
        .map(|value| map_input_value(value))
        .collect::<Vec<_>>();

    let inputs = if inputs.is_empty() {
        vec![InputSource::Stdin]
    } else {
        inputs
    };

    let output = args.output.as_deref().map(parse_output_target);

    let extensions = args
        .extensions
        .as_deref()
        .map(parse_extensions)
        .unwrap_or_default();

    RemoveZwArgs {
        inputs,
        output,
        in_place: args.in_place,
        recursive: args.recursive,
        extensions,
        verbose: args.common.verbose,
        dry_run: args.dry_run,
        check: args.check,
        fail_on_skip: args.fail_on_skip,
        keep_bom: args.keep_bom,
    }
}

/// Prints the tool's runtime configuration, shown under `--app-header`.
///
/// `label: value` lines are rendered through the shared `format_config_item`
/// and the `Inputs:`/`Output:` group lines through `format_config_label`; the
/// nested value bullets have no `label: value` shape, so they print from the
/// `CONFIG_UL_ITEM_LEVEL_3` constant.
fn print_header(args: &RemoveZwArgs) {
    println!("{}", format_config_label("Inputs:"));
    for input in &args.inputs {
        match input {
            InputSource::Stdin => {
                println!("{} stdin (filter mode)", CONFIG_UL_ITEM_LEVEL_3)
            }
            InputSource::File(path) => println!("{} {}", CONFIG_UL_ITEM_LEVEL_3, path.display()),
            InputSource::Directory(path) => {
                println!("{} {} (dir)", CONFIG_UL_ITEM_LEVEL_3, path.display())
            }
        }
    }

    println!("{}", format_config_label("Output:"));
    if args.dry_run {
        println!("{} Dry run (nothing written)", CONFIG_UL_ITEM_LEVEL_3);
    } else if args.check {
        if args.fail_on_skip {
            println!("{} Check (report only; skips fail)", CONFIG_UL_ITEM_LEVEL_3);
        } else {
            println!("{} Check (report only)", CONFIG_UL_ITEM_LEVEL_3);
        }
    } else if args.in_place {
        println!("{} In place", CONFIG_UL_ITEM_LEVEL_3);
    } else if let Some(output) = &args.output {
        match output {
            OutputTarget::Stdout => println!("{} Stdout", CONFIG_UL_ITEM_LEVEL_3),
            OutputTarget::File(path) => println!("{} {}", CONFIG_UL_ITEM_LEVEL_3, path.display()),
        }
    } else if args
        .inputs
        .iter()
        .all(|input| matches!(input, InputSource::Stdin))
    {
        println!("{} Stdout", CONFIG_UL_ITEM_LEVEL_3);
    } else {
        println!("{} Per-file cleaned output", CONFIG_UL_ITEM_LEVEL_3);
    }

    println!("{}", format_config_item("Recursive", args.recursive));
    println!("{}", format_config_item("Keep BOM", args.keep_bom));
    if args.extensions.is_empty() {
        println!("{}", format_config_item("Extensions", "(all)"));
    } else {
        println!(
            "{}",
            format_config_item("Extensions", format!("{:?}", args.extensions))
        );
    }
}

pub(crate) fn validate_args(args: &RemoveZwArgs) -> Result<()> {
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

    if let Some(OutputTarget::File(_)) = args.output
        && (args.inputs.len() > 1
            || args
                .inputs
                .iter()
                .any(|i| matches!(i, InputSource::Directory(_))))
    {
        return Err(anyhow!(
            "--output FILE requires a single file input (use '-' for stdout)"
        ));
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
    use clap::CommandFactory;
    use tempfile::tempdir;

    fn args_with(inputs: Vec<InputSource>) -> RemoveZwArgs {
        RemoveZwArgs {
            inputs,
            output: None,
            in_place: false,
            recursive: false,
            extensions: Vec::new(),
            verbose: false,
            dry_run: false,
            check: false,
            fail_on_skip: false,
            keep_bom: false,
        }
    }

    #[test]
    fn cli_definition_has_no_conflicting_flags() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn check_conflicts_with_write_modes_and_dry_run() {
        // clap rejects these combinations at parse time (its usage-error exit
        // code is pinned at process level in tests/cli_tests.rs).
        for conflicting in [
            ["remove-zw", "--check", "--in-place"],
            ["remove-zw", "--check", "--dry-run"],
        ] {
            assert!(
                CliArgs::try_parse_from(conflicting).is_err(),
                "{conflicting:?} should be rejected"
            );
        }
        assert!(CliArgs::try_parse_from(["remove-zw", "--check", "--output", "x"]).is_err());
    }

    #[test]
    fn fail_on_skip_requires_check() {
        assert!(CliArgs::try_parse_from(["remove-zw", "--fail-on-skip", "in.txt"]).is_err());

        let args = CliArgs::try_parse_from(["remove-zw", "--check", "--fail-on-skip", "in.txt"])
            .expect("valid combination");
        let config = build_args(&args);
        assert!(config.check);
        assert!(config.fail_on_skip);
    }

    #[test]
    fn check_and_keep_bom_flow_into_the_config() {
        let args = CliArgs::try_parse_from(["remove-zw", "--check", "--keep-bom", "in.txt"]);
        let config = build_args(&args.unwrap());
        assert!(config.check);
        assert!(config.keep_bom);
        assert!(config.report_only());

        let plain = build_args(&CliArgs::try_parse_from(["remove-zw", "in.txt"]).unwrap());
        assert!(!plain.check);
        assert!(!plain.keep_bom);
        assert!(!plain.report_only());
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
    fn print_header_covers_dry_run_and_default_modes() {
        // Smoke test: the header prints for a dry-run and a default per-file
        // invocation without panicking.
        let mut dry = args_with(vec![InputSource::Directory(PathBuf::from("docs"))]);
        dry.dry_run = true;
        dry.recursive = true;
        dry.extensions = vec!["txt".to_string()];
        print_header(&dry);

        let default = args_with(vec![InputSource::File(PathBuf::from("a.txt"))]);
        print_header(&default);

        let to_stdout = args_with(vec![InputSource::Stdin]);
        print_header(&to_stdout);

        let mut check = args_with(vec![InputSource::File(PathBuf::from("a.txt"))]);
        check.check = true;
        check.keep_bom = true;
        print_header(&check);
    }
}
