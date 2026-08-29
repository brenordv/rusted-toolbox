use crate::models::SplitArgs;
use anyhow::{Context, Result};
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_utils::constants::CONFIG_UL_ITEM_LEVEL_2;
use common_utils::file_system::get_current_dir;
use std::path::{Path, PathBuf};

/// Split files by the number of lines.
///
/// Split large UTF-8 text or CSV files by line count, preserving an optional CSV header in each part.
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
/// or the output directory cannot be created.
pub fn initialize() -> Result<SplitArgs> {
    let args = CliArgs::parse();

    let config = build_args(&args);

    validate_and_prepare(&config)?;

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| {
            print_header(&config);
        }),
    );

    Ok(config)
}

/// Resolves the parsed CLI arguments into the runtime configuration.
fn build_args(args: &CliArgs) -> SplitArgs {
    let current_working_dir = get_current_dir();

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
/// or the output directory cannot be created.
fn validate_and_prepare(args: &SplitArgs) -> Result<()> {
    if !Path::new(&args.input_file).exists() {
        anyhow::bail!("Input file '{}' does not exist", args.input_file);
    }

    if args.lines_per_file == 0 {
        anyhow::bail!("Lines per file must be greater than 0");
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

/// Prints the tool's runtime configuration, shown under `--app-header`.
fn print_header(args: &SplitArgs) {
    println!("{} Input file: {}", CONFIG_UL_ITEM_LEVEL_2, args.input_file);
    println!("{} Output dir: {}", CONFIG_UL_ITEM_LEVEL_2, args.output_dir);
    println!(
        "{} Lines per file: {}",
        CONFIG_UL_ITEM_LEVEL_2, args.lines_per_file
    );
    println!("{} File prefix: {}", CONFIG_UL_ITEM_LEVEL_2, args.prefix);
    println!("{} CSV mode: {}", CONFIG_UL_ITEM_LEVEL_2, args.csv_mode);
    println!(
        "{} Feedback interval: {}",
        CONFIG_UL_ITEM_LEVEL_2, args.feedback_interval
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
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
}
