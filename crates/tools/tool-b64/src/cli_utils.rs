use crate::models::B64Config;
use clap::{builder::NonEmptyStringValueParser, Arg, ArgAction, Command};
use shared::command_line::cli_builder::CommandExt;

/// Parses command-line arguments and returns the runtime configuration.
pub fn get_cli_arguments() -> B64Config {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            "Encode or decode data using Base64.",
            "Encode or decode data using Base64. With no FILE, or when FILE is -, read standard input.",
        )
        .arg(
            Arg::new("decode")
                .short('d')
                .long("decode")
                .action(ArgAction::SetTrue)
                .help("Decode Base64 input to binary"),
        )
        .arg(
            Arg::new("wrap")
                .short('w')
                .long("wrap")
                .value_name("COLS")
                .value_parser(clap::value_parser!(usize))
                .action(ArgAction::Set)
                .overrides_with("bsd-wrap")
                .help("Wrap encoded lines at COLS (default 76; 0 disables)"),
        )
        .arg(
            Arg::new("bsd-wrap")
                .short('b')
                .value_name("COLS")
                .value_parser(clap::value_parser!(usize))
                .action(ArgAction::Set)
                .overrides_with("wrap")
                .help("BSD/macOS alias for --wrap"),
        )
        .arg(
            Arg::new("ignore-garbage")
                .short('i')
                .long("ignore-garbage")
                .action(ArgAction::SetTrue)
                .help("When decoding, ignore non-Base64 characters"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FILE")
                .value_parser(NonEmptyStringValueParser::new())
                .help("Write output to FILE instead of stdout"),
        )
        .arg(
            Arg::new("text")
                .short('t')
                .long("text")
                .value_name("INPUT")
                .value_parser(NonEmptyStringValueParser::new())
                .conflicts_with_all(["file", "input"])
                .help("Treat INPUT as literal text"),
        )
        .arg(
            Arg::new("file")
                .short('f')
                .long("file")
                .value_name("INPUT")
                .value_parser(NonEmptyStringValueParser::new())
                .conflicts_with_all(["text", "input"])
                .help("Treat INPUT as a file path"),
        )
        .arg(
            Arg::new("input")
                .value_name("INPUT")
                .num_args(0..=1)
                .value_parser(NonEmptyStringValueParser::new())
                .conflicts_with_all(["text", "file"])
                .help("Auto-detect INPUT as file (if it exists) or literal text"),
        )
        .get_matches();

    B64Config::from_matches(&matches)
}
