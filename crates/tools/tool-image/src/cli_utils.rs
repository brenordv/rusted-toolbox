use crate::models::{ImageConfig, ResizeFilter, ResizeSpec};
use crate::string_traits::{StringExt, SUPPORTED_FORMAT_NAMES};
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::header_format::format_config_item;
use common_cli::tool_exit_helpers::exit_error;
use common_file_utils::file_system::list_all_files_recursively;
use common_utils::constants::{CONFIG_UL_ITEM_LEVEL_2, CONFIG_UL_ITEM_LEVEL_3};
use image::imageops::FilterType;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};

/// Quick way for editing images from the terminal.
///
/// CLI tool to perform some quick image edits for simple actions like converting, resizing,
/// and grayscale conversion.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
pub struct CliArgs {
    /// Input files to process.
    #[arg(num_args = 0.., required = false)]
    pub input_files: Vec<String>,

    /// Resize by percentage or by size. (Examples: 50, 12.5%, 640,480, 640.5,480.25)
    #[arg(short='r', long="resize", value_name="RESIZE", value_parser=parse_resize)]
    pub resize: Option<ResizeSpec>,

    /// Converts the image to grayscale.
    #[arg(short = 'g', long = "grayscale", required = false)]
    pub grayscale: bool,

    /// Convert the image to the specified format.
    #[arg(short = 'c', long = "convert", required = false, value_parser = parse_convert_format)]
    pub convert: Option<String>,

    /// Encoding quality for JPEG and AVIF output (1-100, 100 is best). Other formats ignore it.
    #[arg(short = 'q', long = "quality", value_parser = clap::value_parser!(u8).range(1..=100))]
    pub quality: Option<u8>,

    /// Resize filter to use with --resize. (Default: lanczos3)
    #[arg(short = 'f', long = "filter", value_enum, requires = "resize")]
    pub filter: Option<ResizeFilter>,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Validates a `--convert` value against the supported format names,
/// case-insensitively, so an unknown format is a clap parse error. Returns the
/// value as typed; `to_image_format` lowercases it when mapping.
fn parse_convert_format(value: &str) -> Result<String, String> {
    if SUPPORTED_FORMAT_NAMES.contains(&value.to_lowercase().as_str()) {
        Ok(value.to_string())
    } else {
        Err(format!(
            "unsupported image format '{}' (supported: {})",
            value,
            SUPPORTED_FORMAT_NAMES.join(", ")
        ))
    }
}

/// Prints the tool's runtime configuration, shown under `--app-header`.
///
/// `label: value` lines render through the shared `format_config_item`; the
/// `Files` group line and its nested file bullets have no `label: value`
/// shape, so they print from the `CONFIG_UL_*` constants.
fn print_runtime_info(args: &CliArgs) {
    println!("{} Files", CONFIG_UL_ITEM_LEVEL_2);
    for file in args.input_files.iter() {
        println!("{} {:?}", CONFIG_UL_ITEM_LEVEL_3, file);
    }

    if let Some(resize) = &args.resize {
        println!("{}", format_config_item("Resize", resize));
    }

    if args.grayscale {
        println!("{}", format_config_item("Grayscale", "true"));
    }

    if let Some(convert) = &args.convert {
        println!(
            "{}",
            format_config_item("Convert", format!("{:?}", convert))
        );
    }

    if let Some(quality) = args.quality {
        println!("{}", format_config_item("Quality", quality));
    }

    if let Some(filter) = args.filter {
        println!(
            "{}",
            format_config_item("Resize filter", format!("{:?}", filter))
        );
    }
}

pub fn initialize() -> ImageConfig {
    let args = CliArgs::parse();

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| {
            print_runtime_info(&args);
        }),
    );

    let input_files = match expand_input_paths(&args.input_files) {
        Ok(files) => files,
        Err(e) => {
            error!("{e}");
            exit_error();
        }
    };

    let convert = args.convert.map(|c| c.to_image_format());

    ImageConfig {
        input_files,
        resize: args.resize,
        grayscale: args.grayscale,
        convert,
        quality: args.quality,
        filter: args
            .filter
            .map(ResizeFilter::to_filter_type)
            .unwrap_or(FilterType::Lanczos3),
    }
}

fn expand_input_paths(paths: &Vec<String>) -> Result<Vec<PathBuf>> {
    let mut expanded_paths = HashSet::new();

    for path in paths {
        let candidate = Path::new(path);

        if let Some((dir, extension)) = as_extension_glob(candidate) {
            match collect_dir_by_extension(&dir, &extension) {
                Ok(matches) => expanded_paths.extend(matches),
                Err(e) => error!("[Skipping] {} | Reason: {}", path, e),
            }
            continue;
        }

        if candidate.is_dir() {
            debug!("Expanding directory: {}", candidate.display());
            expanded_paths.extend(list_all_files_recursively(candidate));
            continue;
        }

        expanded_paths.insert(candidate.to_path_buf());
    }

    let filtered_paths: Vec<PathBuf> = expanded_paths
        .into_iter()
        .filter(|file| {
            if file.is_dir() {
                error!("[Skipping] {} | Reason: Directory", file.display());
                return false;
            }

            if !file.exists() {
                error!(
                    "[Skipping] {} | Reason: File does not exist",
                    file.display()
                );
                return false;
            }

            if !is_supported_image_file(file) {
                error!(
                    "[Skipping] {} | Reason: Unsupported file type",
                    file.display()
                );
                return false;
            }

            true
        })
        .collect();

    if filtered_paths.is_empty() {
        return Err(anyhow!(
            "No supported image files found. Nothing to work with."
        ));
    }

    info!("Found {} supported image files.", filtered_paths.len());
    Ok(filtered_paths)
}

/// Detects a `*.<ext>` glob and returns the directory to scan together with the target extension.
///
/// The directory defaults to `.` when the pattern carries no directory component, so `*.png`
/// scans the current directory while `photos/*.png` scans `photos`. Matching is non-recursive,
/// mirroring how POSIX shells expand `*.png`; shells that do not expand globs (PowerShell, cmd)
/// hand the literal pattern through to here instead.
fn as_extension_glob(path: &Path) -> Option<(PathBuf, String)> {
    let file_name = path.file_name()?.to_str()?;
    let extension = file_name.strip_prefix("*.")?;
    if extension.is_empty() || extension.contains('*') || extension.contains('?') {
        return None;
    }

    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };

    Some((dir, extension.to_string()))
}

/// Collects the entries of `dir` whose extension matches `extension`, case-insensitively.
fn collect_dir_by_extension(dir: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("Cannot read directory '{}'", dir.display()))?;

    let matches = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|entry_path| {
            entry_path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case(extension))
                .unwrap_or(false)
        })
        .collect();

    Ok(matches)
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

fn is_supported_image_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext = ext.to_lowercase();
        matches!(
            ext.as_str(),
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "tiff" | "tif" | "bmp"
        )
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn convert_rejects_unknown_format_at_parse() {
        let result = CliArgs::try_parse_from(["imgx", "--convert", "bogus", "img.png"]);
        assert!(result.is_err());
    }

    #[test]
    fn convert_accepts_supported_format_case_insensitively() {
        let args = CliArgs::try_parse_from(["imgx", "--convert", "PNG", "img.png"]).unwrap();
        assert_eq!(args.convert.as_deref(), Some("PNG"));
    }

    #[test]
    fn cli_definition_is_consistent() {
        use clap::CommandFactory;
        CliArgs::command().debug_assert();
    }

    #[test]
    fn quality_accepts_bounds_and_rejects_outside() {
        assert!(CliArgs::try_parse_from(["imgx", "--quality", "1", "img.png"]).is_ok());
        assert!(CliArgs::try_parse_from(["imgx", "--quality", "100", "img.png"]).is_ok());
        assert!(CliArgs::try_parse_from(["imgx", "--quality", "0", "img.png"]).is_err());
        assert!(CliArgs::try_parse_from(["imgx", "--quality", "101", "img.png"]).is_err());
        assert!(CliArgs::try_parse_from(["imgx", "--quality", "9.5", "img.png"]).is_err());
    }

    #[test]
    fn filter_rejects_unknown_value() {
        let result = CliArgs::try_parse_from(["imgx", "-r", "50", "--filter", "bogus", "img.png"]);
        assert!(result.is_err());
    }

    #[test]
    fn filter_requires_resize() {
        assert!(CliArgs::try_parse_from(["imgx", "--filter", "nearest", "img.png"]).is_err());
    }

    #[test]
    fn filter_parses_the_documented_names() {
        let args =
            CliArgs::try_parse_from(["imgx", "-r", "50", "-f", "catmullrom", "img.png"]).unwrap();
        assert_eq!(args.filter, Some(ResizeFilter::Catmullrom));

        let args =
            CliArgs::try_parse_from(["imgx", "-r", "50", "-f", "nearest", "img.png"]).unwrap();
        assert_eq!(args.filter, Some(ResizeFilter::Nearest));
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
    fn as_extension_glob_detects_bare_pattern() {
        let (dir, ext) = as_extension_glob(Path::new("*.png")).unwrap();
        assert_eq!(dir, PathBuf::from("."));
        assert_eq!(ext, "png");
    }

    #[test]
    fn as_extension_glob_detects_pattern_with_directory() {
        let (dir, ext) = as_extension_glob(Path::new("photos/*.jpg")).unwrap();
        assert_eq!(dir, PathBuf::from("photos"));
        assert_eq!(ext, "jpg");
    }

    #[test]
    fn as_extension_glob_ignores_plain_paths() {
        assert!(as_extension_glob(Path::new("photo.png")).is_none());
        assert!(as_extension_glob(Path::new("photos")).is_none());
    }

    #[test]
    fn expand_input_paths_walks_a_directory_for_supported_images() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.png"), b"x").unwrap();
        std::fs::write(dir.path().join("b.jpg"), b"x").unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();

        let result = expand_input_paths(&vec![dir.path().display().to_string()]).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|p| is_supported_image_file(p)));
    }

    #[test]
    fn expand_input_paths_matches_only_the_glob_extension() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.png"), b"x").unwrap();
        std::fs::write(dir.path().join("b.png"), b"x").unwrap();
        std::fs::write(dir.path().join("c.jpg"), b"x").unwrap();

        let pattern = format!("{}/*.png", dir.path().display());
        let result = expand_input_paths(&vec![pattern]).unwrap();

        assert_eq!(result.len(), 2);
        assert!(result
            .iter()
            .all(|p| p.extension().and_then(|e| e.to_str()) == Some("png")));
    }

    #[test]
    fn expand_input_paths_keeps_a_named_image_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("photo.png");
        std::fs::write(&file, b"x").unwrap();

        let result = expand_input_paths(&vec![file.display().to_string()]).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].file_name().unwrap(), "photo.png");
    }

    #[test]
    fn expand_input_paths_errors_when_nothing_is_supported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();

        let result = expand_input_paths(&vec![dir.path().display().to_string()]);

        assert!(result.is_err());
    }
}
