use clap::ArgMatches;
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
    /// Creates the runtime configuration from clap matches.
    pub fn from_matches(matches: &ArgMatches) -> Self {
        let mode = if matches.get_flag("decode") {
            B64Mode::Decode
        } else {
            B64Mode::Encode
        };

        let wrap_value = matches
            .get_one::<usize>("wrap")
            .copied()
            .or_else(|| matches.get_one::<usize>("bsd-wrap").copied())
            .unwrap_or(76);

        let wrap_columns = NonZeroUsize::new(wrap_value);

        let ignore_garbage = matches.get_flag("ignore-garbage");

        let input = determine_input_source(matches);

        let output = matches
            .get_one::<String>("output")
            .map(|value| value.as_str())
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

fn determine_input_source(matches: &ArgMatches) -> InputSource {
    if let Some(text) = matches.get_one::<String>("text") {
        return InputSource::Text(text.to_string());
    }

    if let Some(file) = matches.get_one::<String>("file") {
        return InputSource::File(PathBuf::from(file));
    }

    if let Some(input) = matches.get_one::<String>("input") {
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
    use clap::ArgMatches;

    fn matches_from(args: &[&str]) -> ArgMatches {
        clap::Command::new("b64")
            .disable_help_flag(true)
            .disable_version_flag(true)
            .arg(
                clap::Arg::new("decode")
                    .short('d')
                    .long("decode")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("wrap")
                    .short('w')
                    .long("wrap")
                    .value_parser(clap::value_parser!(usize)),
            )
            .arg(
                clap::Arg::new("bsd-wrap")
                    .short('b')
                    .value_parser(clap::value_parser!(usize)),
            )
            .arg(
                clap::Arg::new("ignore-garbage")
                    .short('i')
                    .long("ignore-garbage")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                clap::Arg::new("output")
                    .short('o')
                    .long("output")
                    .value_parser(clap::builder::NonEmptyStringValueParser::new()),
            )
            .arg(
                clap::Arg::new("text")
                    .short('t')
                    .long("text")
                    .value_parser(clap::builder::NonEmptyStringValueParser::new())
                    .conflicts_with_all(["file", "input"]),
            )
            .arg(
                clap::Arg::new("file")
                    .short('f')
                    .long("file")
                    .value_parser(clap::builder::NonEmptyStringValueParser::new())
                    .conflicts_with_all(["text", "input"]),
            )
            .arg(
                clap::Arg::new("input")
                    .value_parser(clap::builder::NonEmptyStringValueParser::new())
                    .num_args(0..=1)
                    .conflicts_with_all(["text", "file"]),
            )
            .try_get_matches_from(args)
            .expect("valid args")
    }

    #[test]
    fn infer_stdin_with_dash() {
        let matches = matches_from(&["b64", "-"]);
        let config = B64Config::from_matches(&matches);
        assert!(matches!(config.input, InputSource::Stdin));
    }

    #[test]
    fn infer_text_when_no_file_exists() {
        let matches = matches_from(&["b64", "not_a_file_hopefully.txt"]);
        let config = B64Config::from_matches(&matches);
        assert!(matches!(
            config.input,
            InputSource::Text(ref text) if text == "not_a_file_hopefully.txt"
        ));
    }

    #[test]
    fn force_text_with_flag() {
        let matches = matches_from(&["b64", "--text", "hello"]);
        let config = B64Config::from_matches(&matches);
        assert!(matches!(
            config.input,
            InputSource::Text(ref text) if text == "hello"
        ));
    }

    #[test]
    fn force_file_with_flag() {
        let matches = matches_from(&["b64", "--file", "Cargo.toml"]);
        let config = B64Config::from_matches(&matches);
        assert!(matches!(
            config.input,
            InputSource::File(ref path) if path == &PathBuf::from("Cargo.toml")
        ));
    }
}
