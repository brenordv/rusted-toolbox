use crate::models::SplitArgs;
use anyhow::{Context, Result};
use clap::Parser;
use common_cli::broken_pipe::{BrokenPipe, flush_out, write_out};
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::header_format::format_config_item;
use common_utils::file_system::get_current_dir;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Split files by the number of lines.
///
/// Split large CSV or UTF-8 text files by line count, preserving an optional CSV header in each part.
#[derive(Parser, Debug)]
#[command(about, long_about, version)]
struct CliArgs {
    /// Path to the input file
    #[arg(short = 'f', long = "file", value_name = "FILE")]
    pub file: String,

    /// Output directory. If not set, uses the same directory as the input file
    #[arg(short = 'o', long = "output-dir", value_name = "DIR")]
    pub output_dir: Option<String>,

    /// Number of lines per file
    #[arg(short = 'l', long = "lines-per-file", default_value_t = 100)]
    pub lines_per_file: usize,

    /// Prefix for the output files
    #[arg(short = 'p', long = "file-prefix", default_value = "split")]
    pub file_prefix: String,

    /// Interval between feedback updates, in number of lines
    #[arg(short = 'i', long = "feedback-interval", default_value_t = 100)]
    pub feedback_interval: usize,

    /// Use the first line of the input as a header and repeat it in each output file (not counted toward lines per file)
    #[arg(short = 'c', long = "csv-mode")]
    pub csv_mode: bool,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Parses command-line arguments and returns the runtime configuration.
///
/// Relative input and output paths are resolved against the current working directory,
/// and the output directory is created if it does not exist.
///
/// # Errors
/// Returns an error when the input file does not exist, `--lines-per-file` is zero,
/// `--feedback-interval` is zero, or the output directory cannot be created.
pub fn initialize() -> Result<SplitArgs> {
    let args = CliArgs::parse();

    let config = build_args(&args, &get_current_dir());

    // Logging boots before validation, so validation failures are reported
    // through the subscriber by the entrypoint instead of a pre-boot print.
    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| {
            print_header(&config);
        }),
    );

    validate_and_prepare(&config)?;

    Ok(config)
}

/// Resolves the parsed CLI arguments into the runtime configuration. Relative
/// paths are resolved against `base_dir`.
fn build_args(args: &CliArgs, base_dir: &Path) -> SplitArgs {
    let current_working_dir = base_dir.to_path_buf();

    let input_file_path = PathBuf::from(&args.file);
    let input_file = if input_file_path.is_absolute() {
        input_file_path
    } else {
        current_working_dir.join(input_file_path)
    };

    let output_dir = match &args.output_dir {
        Some(dir) => {
            let output_dir_path = PathBuf::from(dir);
            if output_dir_path.is_absolute() {
                output_dir_path.to_string_lossy().to_string()
            } else {
                current_working_dir
                    .join(output_dir_path)
                    .to_string_lossy()
                    .to_string()
            }
        }
        None => input_file
            .parent()
            .unwrap_or(&current_working_dir)
            .to_string_lossy()
            .to_string(),
    };

    let input_filename_without_extension = input_file
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    SplitArgs {
        input_file: input_file.to_string_lossy().to_string(),
        output_dir,
        input_filename_without_extension,
        lines_per_file: args.lines_per_file,
        prefix: args.file_prefix.clone(),
        csv_mode: args.csv_mode,
        feedback_interval: args.feedback_interval,
    }
}

/// Validates the resolved configuration and prepares the output directory.
///
/// # Errors
/// Returns an error when the input file does not exist, `lines_per_file` is zero,
/// `feedback_interval` is zero, or the output directory cannot be created.
fn validate_and_prepare(args: &SplitArgs) -> Result<()> {
    if !Path::new(&args.input_file).exists() {
        anyhow::bail!("Input file '{}' does not exist", args.input_file);
    }

    if args.lines_per_file == 0 {
        anyhow::bail!("Lines per file must be greater than 0");
    }

    if args.feedback_interval == 0 {
        anyhow::bail!("Feedback interval must be greater than 0");
    }

    let output_dir = PathBuf::from(&args.output_dir);
    if !output_dir.exists() {
        std::fs::create_dir_all(&output_dir).with_context(|| {
            format!(
                "Failed to create output directory '{}'",
                output_dir.display()
            )
        })?;
    }

    Ok(())
}

/// Writes the tool's runtime-config lines (shown under `--app-header`) to
/// `output`, mapping a closed pipe to the shared [`BrokenPipe`] marker.
///
/// # Errors
/// Fails with the [`BrokenPipe`] marker when the consumer closed the pipe,
/// or with the underlying I/O error for any other write or flush failure.
fn write_header(output: &mut impl Write, args: &SplitArgs) -> Result<()> {
    let lines = [
        format_config_item("Input file", &args.input_file),
        format_config_item("Output dir", &args.output_dir),
        format_config_item("Lines per file", args.lines_per_file),
        format_config_item("File prefix", &args.prefix),
        format_config_item("CSV mode", args.csv_mode),
        format_config_item("Feedback interval", args.feedback_interval),
    ];
    for line in lines {
        write_out(output, line.as_bytes())?;
        write_out(output, b"\n")?;
    }
    flush_out(output)
}

/// Prints the tool's runtime configuration, shown under `--app-header`,
/// without ever failing the run: a closed consumer is a debug note, any
/// other stdout failure a warning.
fn print_header(args: &SplitArgs) {
    let mut stdout = std::io::stdout();
    if let Err(e) = write_header(&mut stdout, args) {
        if e.is::<BrokenPipe>() {
            debug!("Header output skipped: stdout closed by the consumer");
        } else {
            warn!("Cannot write the header to stdout: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use common_cli::test_writers::{ClosedPipe, FailingFlush};
    use std::fs;
    use tempfile::tempdir;

    fn sample_args(input_file: String, output_dir: String) -> SplitArgs {
        SplitArgs {
            input_file,
            output_dir,
            input_filename_without_extension: "input".to_string(),
            lines_per_file: 100,
            prefix: "split".to_string(),
            csv_mode: false,
            feedback_interval: 100,
        }
    }

    #[test]
    fn cli_definition_has_no_conflicting_flags() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn print_header_smoke() {
        let args = sample_args("in.txt".to_string(), "out".to_string());
        print_header(&args);
    }

    #[test]
    fn write_header_pins_the_six_config_lines() {
        let args = sample_args("in.txt".to_string(), "out".to_string());
        let mut out: Vec<u8> = Vec::new();

        write_header(&mut out, &args).unwrap();

        let expected = concat!(
            "  - Input file: in.txt\n",
            "  - Output dir: out\n",
            "  - Lines per file: 100\n",
            "  - File prefix: split\n",
            "  - CSV mode: false\n",
            "  - Feedback interval: 100\n",
        );
        assert_eq!(String::from_utf8(out).unwrap(), expected);
    }

    #[test]
    fn write_header_maps_a_closed_pipe_to_the_marker() {
        let args = sample_args("in.txt".to_string(), "out".to_string());

        let error = write_header(&mut ClosedPipe, &args).unwrap_err();

        assert!(error.is::<BrokenPipe>());
    }

    #[test]
    fn write_header_keeps_other_errors_ordinary() {
        let args = sample_args("in.txt".to_string(), "out".to_string());

        let error = write_header(&mut FailingFlush, &args).unwrap_err();

        assert!(!error.is::<BrokenPipe>());
    }

    #[test]
    fn validate_and_prepare_creates_missing_output_dir() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("input.txt");
        fs::write(&input, "a\n").unwrap();
        let outdir = dir.path().join("nested").join("out");
        let args = sample_args(
            input.to_string_lossy().to_string(),
            outdir.to_string_lossy().to_string(),
        );

        validate_and_prepare(&args).unwrap();

        assert!(outdir.exists());
    }

    #[test]
    fn validate_and_prepare_rejects_missing_input() {
        let dir = tempdir().unwrap();
        let args = sample_args(
            dir.path().join("nope.txt").to_string_lossy().to_string(),
            dir.path().to_string_lossy().to_string(),
        );

        assert!(validate_and_prepare(&args).is_err());
    }

    #[test]
    fn validate_and_prepare_rejects_zero_lines() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("input.txt");
        fs::write(&input, "a\n").unwrap();
        let mut args = sample_args(
            input.to_string_lossy().to_string(),
            dir.path().to_string_lossy().to_string(),
        );
        args.lines_per_file = 0;

        assert!(validate_and_prepare(&args).is_err());
    }

    #[test]
    fn validate_and_prepare_rejects_zero_feedback_interval() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("input.txt");
        fs::write(&input, "a\n").unwrap();
        let mut args = sample_args(
            input.to_string_lossy().to_string(),
            dir.path().to_string_lossy().to_string(),
        );
        args.feedback_interval = 0;

        assert!(validate_and_prepare(&args).is_err());
    }

    fn cli_args_for(file: &str, output_dir: Option<&str>) -> CliArgs {
        let mut argv = vec!["csv-split", "-f", file];
        if let Some(dir) = output_dir {
            argv.push("-o");
            argv.push(dir);
        }
        CliArgs::try_parse_from(argv).unwrap()
    }

    #[test]
    fn build_args_joins_relative_input_onto_the_base_dir() {
        let base = tempdir().unwrap();
        let args = cli_args_for("data/input.txt", None);

        let config = build_args(&args, base.path());

        assert_eq!(
            PathBuf::from(&config.input_file),
            base.path().join("data").join("input.txt")
        );
    }

    #[test]
    fn build_args_defaults_output_dir_to_the_input_parent() {
        let base = tempdir().unwrap();
        let args = cli_args_for("data/input.txt", None);

        let config = build_args(&args, base.path());

        assert_eq!(PathBuf::from(&config.output_dir), base.path().join("data"));
    }

    #[test]
    fn build_args_passes_absolute_paths_through() {
        let base = tempdir().unwrap();
        let elsewhere = tempdir().unwrap();
        let target = tempdir().unwrap();
        let absolute_input = elsewhere.path().join("input.txt");
        let args = cli_args_for(
            absolute_input.to_str().unwrap(),
            Some(target.path().to_str().unwrap()),
        );

        let config = build_args(&args, base.path());

        assert_eq!(PathBuf::from(&config.input_file), absolute_input);
        assert_eq!(PathBuf::from(&config.output_dir), target.path());
        assert_eq!(config.input_filename_without_extension, "input");
    }
}
