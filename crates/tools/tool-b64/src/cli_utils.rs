use crate::models::B64Config;
use clap::{builder::NonEmptyStringValueParser, Parser};
use common_cli::common_tool_args::CommonToolArgs;

/// Encode or decode data using Base64.
///
/// Encode or decode data using Base64. With no FILE, or when FILE is -, read standard input.
#[derive(Parser, Debug)]
#[command(about, long_about, version)]
pub struct CliArgs {
    /// Decode Base64 input to binary
    #[arg(short = 'd', long = "decode", default_value_t = false)]
    pub decode: bool,

    /// Wrap encoded lines at COLS (default 76; 0 disables. -b for BSD/macOS alias)
    #[arg(
        short = 'w',
        long = "wrap",
        visible_short_alias = 'b',
        value_name = "COLS",
        default_value_t = 76
    )]
    pub wrap: usize,

    /// When decoding, ignore non-Base64 characters
    #[arg(short = 'i', long = "ignore-garbage", default_value_t = false)]
    pub ignore_garbage: bool,

    /// Write output to FILE instead of stdout
    #[arg(short = 'o', long = "output", value_name = "FILE")]
    pub output: Option<String>,

    /// Treat INPUT as literal text
    #[arg(
        short = 't',
        long = "text",
        value_name = "INPUT",
        value_parser(NonEmptyStringValueParser::new()),
        conflicts_with = "input_file"
    )]
    pub input_text: Option<String>,

    /// Treat INPUT as a file path
    #[arg(
        short = 'f',
        long = "file",
        value_name = "INPUT",
        value_parser(NonEmptyStringValueParser::new()),
        conflicts_with = "input_text"
    )]
    pub input_file: Option<String>,

    #[arg(
        value_name = "INPUT",
        value_parser(NonEmptyStringValueParser::new()),
        conflicts_with_all = ["input_text", "input_file"]
    )]
    pub input: Option<String>,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Parses command-line arguments and returns the runtime configuration.
pub fn initialize() -> B64Config {
    let args = CliArgs::parse();

    args.common
        .app_boot_up_headerless(env!("CARGO_PKG_NAME"), false);

    B64Config::from_args(&args)
}
