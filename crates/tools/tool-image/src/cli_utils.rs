use crate::models::{EditArgs, ResizeSpec};
use crate::string_traits::StringExt;
use anyhow::Result;
use clap::{Arg, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;

pub fn print_runtime_info(args: &EditArgs) {
    println!("Image v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);

    println!("- Files");
    for file in args.input_files.iter() {
        println!("  - {:?}", file);
    }

    if let Some(resize) = &args.resize {
        println!("- Resize: {}", resize);
    }

    if args.grayscale {
        println!("- Grayscale: true");
    }

    if let Some(convert) = &args.convert {
        println!("- Convert: {:?}", convert);
    }

    println!();
}

pub fn validate_args(args: &EditArgs) -> Result<()> {
    if args.input_files.is_empty() {
        return Err(anyhow::anyhow!("No input files provided"));
    };

    if args.input_files.iter().any(|file| !file.exists()) {
        return Err(anyhow::anyhow!("Some of the input file does not exist"));
    }

    Ok(())
}

pub fn get_cli_arguments() -> EditArgs {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_DESCRIPTION"),
            "Cli tool to perform some quick image edits.",
        )
        .arg(
            Arg::new("input-files")
                .help("Input files to process")
                .num_args(0..)
                .required(false)
                .action(clap::ArgAction::Append),
        )
        .arg(
            Arg::new("resize")
                .long("resize")
                .short('r')
                .value_name("RESIZE")
                .value_parser(clap::builder::ValueParser::new(parse_resize))
                .help(
                    "Resize by percentage or by size. (Examples: 50, 12.5%, 640,480, 640.5,480.25)",
                ),
        )
        .arg(
            Arg::new("grayscale")
                .long("grayscale")
                .short('g')
                .action(clap::ArgAction::SetTrue)
                .help("Converts the image to grayscale. (Default: false)"),
        )
        .arg(
            Arg::new("convert")
                .long("convert")
                .short('c')
                .value_name("FORMAT")
                .value_parser(clap::value_parser!(String))
                .help("Convert the image to the specified format"),
        )
        .get_matches();

    let convert = matches
        .get_one::<String>("convert")
        .map(|convert| convert.to_image_format());

    EditArgs {
        input_files: matches
            .get_many::<String>("input-files")
            .unwrap_or_default()
            .map(|s| s.into())
            .collect(),
        resize: matches.get_one::<ResizeSpec>("resize").cloned(),
        grayscale: matches.get_flag("grayscale"),
        convert,
    }
}

fn parse_resize(value: &str) -> Result<ResizeSpec, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("Resize value cannot be empty".to_string());
    }

    if let Some((width_str, height_str)) = trimmed.split_once(',') {
        let width = parse_positive_decimal(width_str, "width")?;
        let height = parse_positive_decimal(height_str, "height")?;
        return Ok(ResizeSpec::Dimensions { width, height });
    }

    let percent_str = trimmed.strip_suffix('%').unwrap_or(trimmed);
    let percent = parse_positive_decimal(percent_str, "percent")?;
    Ok(ResizeSpec::Percent(percent))
}

fn parse_positive_decimal(value: &str, label: &str) -> Result<f64, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("Resize {} cannot be empty", label));
    }

    let parsed: f64 = trimmed.parse().map_err(|_| {
        format!(
            "Invalid resize {}: '{}'. Expected a decimal number.",
            label, trimmed
        )
    })?;

    if parsed <= 0.0 {
        return Err(format!(
            "Invalid resize {}: '{}'. Value must be greater than 0.",
            label, trimmed
        ));
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn edit_args(input_files: Vec<PathBuf>) -> EditArgs {
        EditArgs {
            input_files,
            resize: None,
            grayscale: false,
            convert: None,
        }
    }

    #[test]
    fn parse_resize_plain_number_is_percent() {
        let spec = parse_resize("50").unwrap();
        assert!(matches!(spec, ResizeSpec::Percent(p) if (p - 50.0).abs() < 1e-9));
    }

    #[test]
    fn parse_resize_with_percent_sign() {
        let spec = parse_resize("12.5%").unwrap();
        assert!(matches!(spec, ResizeSpec::Percent(p) if (p - 12.5).abs() < 1e-9));
    }

    #[test]
    fn parse_resize_dimensions() {
        let spec = parse_resize("640,480").unwrap();
        assert!(matches!(
            spec,
            ResizeSpec::Dimensions { width, height }
                if (width - 640.0).abs() < 1e-9 && (height - 480.0).abs() < 1e-9
        ));
    }

    #[test]
    fn parse_resize_empty_is_error() {
        assert!(parse_resize("   ").is_err());
    }

    #[test]
    fn parse_resize_non_numeric_is_error() {
        assert!(parse_resize("abc").is_err());
    }

    #[test]
    fn parse_resize_rejects_zero_and_negative() {
        assert!(parse_resize("0").is_err());
        assert!(parse_resize("-5").is_err());
    }

    #[test]
    fn parse_positive_decimal_accepts_valid() {
        assert!(parse_positive_decimal("3.5", "width").is_ok());
    }

    #[test]
    fn parse_positive_decimal_rejects_empty() {
        assert!(parse_positive_decimal("", "width").is_err());
    }

    #[test]
    fn validate_args_rejects_empty_input() {
        assert!(validate_args(&edit_args(vec![])).is_err());
    }

    #[test]
    fn validate_args_rejects_missing_file() {
        let args = edit_args(vec![PathBuf::from("definitely-not-a-real-file.xyz")]);
        assert!(validate_args(&args).is_err());
    }

    #[test]
    fn validate_args_accepts_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("image.png");
        std::fs::write(&file_path, b"data").unwrap();

        let args = edit_args(vec![file_path]);

        assert!(validate_args(&args).is_ok());
    }
}
