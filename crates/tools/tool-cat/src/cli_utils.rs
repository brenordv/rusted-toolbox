use crate::models::CatArgs;
use clap::{Arg, Command};
use shared::command_line::cli_builder::CommandExt;

/// Parses command-line arguments for cat application.
///
/// Supports Unix cat flags for line numbering, character visualization, and formatting.
/// Reads from stdin if no files provided.
///
/// # Errors
/// Returns error if argument parsing fails
///
/// # Supported Options
/// - `-A, --show-all`: Show all non-printing characters
/// - `-b, --number-nonblank`: Number non-blank lines
/// - `-e`: Equivalent to -vE
/// - `-E, --show-ends`: Show line endings with $
/// - `-n, --number`: Number all lines
/// - `-s, --squeeze-blank`: Squeeze multiple blank lines
/// - `-t`: Equivalent to -vT
/// - `-T, --show-tabs`: Show tabs as ^I
/// - `-u`: Ignored for compatibility
/// - `-v, --show-nonprinting`: Show non-printing characters
/// - `files`: Files to process (reads from stdin if none)
///
/// # Metadata
///
/// - Name: `CAT_APP_NAME` (constant).
/// - Version: `CAT_VERSION` (constant).
/// - Description: Concatenates files and outputs their contents to the standard output. When no file is
///   specified, it reads from standard input.
///
/// # Dependencies
/// This function uses the `clap` crate for defining and parsing command-line arguments.
///
/// # Notes
/// If no files are provided, the program will default to reading from `stdin`.
pub fn get_cli_arguments() -> CatArgs {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            "Concatenate files and print on the standard output",
            "Mimics the behavior of CAT (the Linux tool).\
            Concatenate FILE(s) to standard output.\n\n\
            With no FILE, or when FILE is -, read standard input.",
        )
        .arg(
            Arg::new("show-all")
                .short('A')
                .long("show-all")
                .action(clap::ArgAction::SetTrue)
                .help("equivalent to -vET"),
        )
        .arg(
            Arg::new("number-nonblank")
                .short('b')
                .long("number-nonblank")
                .action(clap::ArgAction::SetTrue)
                .help("number nonempty output lines, overrides -n"),
        )
        .arg(
            Arg::new("e")
                .short('e')
                .action(clap::ArgAction::SetTrue)
                .help("equivalent to -vE"),
        )
        .arg(
            Arg::new("show-ends")
                .short('E')
                .long("show-ends")
                .action(clap::ArgAction::SetTrue)
                .help("display $ at end of each line"),
        )
        .arg(
            Arg::new("number")
                .short('n')
                .long("number")
                .action(clap::ArgAction::SetTrue)
                .help("number all output lines"),
        )
        .arg(
            Arg::new("squeeze-blank")
                .short('s')
                .long("squeeze-blank")
                .action(clap::ArgAction::SetTrue)
                .help("suppress repeated empty output lines"),
        )
        .arg(
            Arg::new("t")
                .short('t')
                .action(clap::ArgAction::SetTrue)
                .help("equivalent to -vT"),
        )
        .arg(
            Arg::new("show-tabs")
                .short('T')
                .long("show-tabs")
                .action(clap::ArgAction::SetTrue)
                .help("display TAB characters as ^I"),
        )
        .arg(
            Arg::new("u")
                .short('u')
                .action(clap::ArgAction::SetTrue)
                .help("(ignored)"),
        )
        .arg(
            Arg::new("show-nonprinting")
                .short('v')
                .long("show-nonprinting")
                .action(clap::ArgAction::SetTrue)
                .help("use ^ and M- notation, except for LFD and TAB"),
        )
        .arg(
            Arg::new("files")
                .help("Files to display")
                .action(clap::ArgAction::Append)
                .num_args(0..),
        )
        .get_matches();

    CatArgs::parse(&matches)
}
