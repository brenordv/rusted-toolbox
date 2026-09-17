use crate::cli_utils::CliArgs;
use std::fs;
use std::io;
use std::num::NonZeroUsize;
use std::path::PathBuf;

/// Indicates whether the tool should encode or decode.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum B64Mode {
    Encode,
    Decode,
}

/// Source of the input data.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum InputSource {
    Stdin,
    File(PathBuf),
    Text(String),
}

/// Destination for the processed output.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum OutputTarget {
    Stdout,
    File(PathBuf),
}

/// Fully parsed configuration derived from the CLI arguments.
#[derive(Debug, Clone)]
pub struct B64Config {
    pub mode: B64Mode,
    pub wrap_columns: Option<NonZeroUsize>,
    pub ignore_garbage: bool,
    pub input: InputSource,
    pub output: OutputTarget,
}

impl B64Config {
    pub fn from_args(args: &CliArgs) -> B64Config {
        let mode = if args.decode {
            B64Mode::Decode
        } else {
            B64Mode::Encode
        };

        let wrap_columns = NonZeroUsize::new(args.wrap);

        let ignore_garbage = args.ignore_garbage;

        let input = determine_input_source(args);

        let output = args
            .output
            .as_deref()
            .map_or(OutputTarget::Stdout, parse_output_target);

        B64Config {
            mode,
            wrap_columns,
            ignore_garbage,
            input,
            output,
        }
    }
}

fn parse_output_target(path: &str) -> OutputTarget {
    if path == "-" {
        OutputTarget::Stdout
    } else {
        OutputTarget::File(PathBuf::from(path))
    }
}

fn determine_input_source(matches: &CliArgs) -> InputSource {
    if let Some(text) = matches.input_text.as_deref() {
        return InputSource::Text(text.to_string());
    }

    if let Some(file) = matches.input_file.as_deref() {
        return InputSource::File(PathBuf::from(file));
    }

    if let Some(input) = matches.input.as_deref() {
        return infer_input_source(input);
    }

    InputSource::Stdin
}

fn infer_input_source(value: &str) -> InputSource {
    if value == "-" {
        return InputSource::Stdin;
    }

    let path = PathBuf::from(value);

    match fs::metadata(&path) {
        Ok(metadata) => {
            if metadata.is_file() {
                InputSource::File(path)
            } else {
                InputSource::Text(value.to_string())
            }
        }
        Err(err) => {
            if err.kind() == io::ErrorKind::PermissionDenied {
                InputSource::File(path)
            } else {
                InputSource::Text(value.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use tempfile::tempdir;

    fn parse(args: &[&str]) -> B64Config {
        let cli = CliArgs::try_parse_from(args).unwrap();
        B64Config::from_args(&cli)
    }

    #[test]
    fn from_args_decode_flag_selects_mode() {
        assert_eq!(parse(&["b64"]).mode, B64Mode::Encode);
        assert_eq!(parse(&["b64", "-d"]).mode, B64Mode::Decode);
    }

    #[test]
    fn from_args_wrap_columns_parse() {
        assert_eq!(parse(&["b64"]).wrap_columns, NonZeroUsize::new(76));
        assert_eq!(parse(&["b64", "-w", "0"]).wrap_columns, None);
        assert_eq!(
            parse(&["b64", "-b", "40"]).wrap_columns,
            NonZeroUsize::new(40)
        );
    }

    #[test]
    fn from_args_output_target() {
        assert_eq!(parse(&["b64"]).output, OutputTarget::Stdout);
        assert_eq!(parse(&["b64", "-o", "-"]).output, OutputTarget::Stdout);
        assert_eq!(
            parse(&["b64", "-o", "out.txt"]).output,
            OutputTarget::File(PathBuf::from("out.txt"))
        );
    }

    #[test]
    fn from_args_text_flag_is_text_input() {
        assert_eq!(
            parse(&["b64", "-t", "hello"]).input,
            InputSource::Text("hello".to_string())
        );
    }

    #[test]
    fn from_args_file_flag_is_file_input() {
        assert_eq!(
            parse(&["b64", "-f", "some/path"]).input,
            InputSource::File(PathBuf::from("some/path"))
        );
    }

    #[test]
    fn from_args_positional_dash_is_stdin() {
        assert_eq!(parse(&["b64", "-"]).input, InputSource::Stdin);
    }

    #[test]
    fn from_args_positional_missing_path_is_text() {
        assert_eq!(
            parse(&["b64", "b64_nonexistent_input_marker"]).input,
            InputSource::Text("b64_nonexistent_input_marker".to_string())
        );
    }

    #[test]
    fn from_args_positional_existing_file_is_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("real.txt");
        fs::write(&file_path, b"data").unwrap();

        let arg = file_path.to_str().unwrap();
        assert_eq!(
            parse(&["b64", arg]).input,
            InputSource::File(file_path.clone())
        );
    }

    #[test]
    fn from_args_positional_existing_directory_is_text() {
        let dir = tempdir().unwrap();
        let arg = dir.path().to_str().unwrap();

        assert_eq!(
            parse(&["b64", arg]).input,
            InputSource::Text(arg.to_string())
        );
    }
}
