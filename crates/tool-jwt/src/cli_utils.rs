use crate::models::{JwtArgs, JwtPrint};
use clap::{Arg, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;

/// Displays JWT runtime configuration information.
///
/// Shows version, token length, clipboard claim, and print format.
///
/// # Arguments
///
/// * `args` - A reference to a `JwtArgs` struct that contains the runtime arguments and
///   configuration settings for the JWT. This includes:
///  - `token`: A string slice representing the JWT token.
///  - `claim_to_clipboard` (optional): An optional claim that will be copied
///    to the clipboard if provided.
///  - `print`: The selected output format, represented by the `JwtPrint` enum.
///
/// The output format (Pretty, CSV, or JSON) and other details help users interact with
/// the JWT intuitively.
///
/// # Example Output
///
/// The function will print details in a user-friendly format, for instance:
///
/// ```text
/// 🚀 JWT v1.0
/// ---------------------------
/// 📏 Token length: 256
/// 📋 Claim to Clipboard: sub
/// ✨  Print format: Pretty
/// ```
///
/// # Notes
///
/// - Ensure proper initialization of the `args` object to avoid runtime errors.
/// - The `DASH_LINE` constant is a predefined separator for formatting output.
///
/// # Panics
/// This function does not implicitly panic, but any inner failures in `println!` or
/// string manipulation (if applicable) might result in runtime errors.
///
/// # Dependencies
/// The function requires the constants `JWT_VERSION` and `DASH_LINE`, as well as the
/// `JwtArgs` and `JwtPrint` types to be defined elsewhere in the codebase.
pub fn print_runtime_info(args: &JwtArgs) {
    println!("JWT v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);
    println!("- Token length: {}", &args.token.len());

    if let Some(copy_to_clipboard) = &args.claim_to_clipboard {
        println!("- Claim to Clipboard: {}", copy_to_clipboard);
    }

    match &args.print {
        JwtPrint::Pretty => println!("- Print format: Pretty"),
        JwtPrint::Csv => println!("- Print format: CSV"),
        JwtPrint::Json => println!("- Print format: JSON"),
    }

    println!();
}

/// Parses command-line arguments into JWT configuration.
///
/// Cleans token input by removing "Bearer" prefix and whitespace.
/// Defaults to pretty print format if not specified.
///
/// # Errors
/// Panics if invalid print format is provided
///
/// # Arguments
///
/// - `--copy-to-clipboard`, `-c <claim>`: (Optional) Specifies the claim that will be copied to the clipboard. Does not work with continuous generation.
/// - `--print`, `-p <format>`: (Optional) Specifies the output format of the decoded data. Valid formats are `pretty`, `csv`, or `json`. Defaults to `pretty` if not specified.
/// - `<token>`: (Required) The JWT token to decode. Supports tokens in any format, ignoring "Bearer" prefixes, line breaks, spaces, and other whitespaces.
///
/// This example decodes the provided JWT in `json` format and copies the `sub` claim's value to the clipboard.
///
/// # Panics
///
/// - Will panic if the `--print` argument provides an invalid output format.
///   Valid formats are `pretty`, `csv`, and `json`.
///
/// # Notes
///
/// - The function makes use of the `clap` crate to parse command line arguments.
/// - The `token` argument allows one or more JWT strings and automatically cleans up formatting inconsistencies
///   such as the "Bearer" keyword or extraneous whitespaces.
pub fn get_cli_arguments() -> JwtArgs {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            "Decodes and prints public JWT tokens data.",
            "Decodes all public info in the JWT tokens, and can either pretty-print, print the csv or json format. Optionally, can copy one of the claims to the clipboard.")
        .arg(Arg::new("copy-to-clipboard")
            .long("copy-to-clipboard")
            .short('c')
            .num_args(1)
            .help("If set, will copy the value to the clipboard. Does not work in conjunction with continuous generation. (Default: false)"))
        .arg(Arg::new("print")
            .long("print")
            .short('p')
            .num_args(1)
            .help("Defines how to print the decoded data. Valid values: pretty, csv, json."))
        .arg(Arg::new("token")
            .num_args(1..)
            .required(true)
            .help("The token that will be decoded. It does not matter if it has the word Bearer or any line breaks."))
        .get_matches();

    let claim_to_clipboard = matches.get_one::<String>("copy-to-clipboard").cloned();

    let print = if let Some(print_arg) = matches.get_one::<String>("print") {
        JwtPrint::from_str(print_arg).expect("Invalid print format.")
    } else {
        JwtPrint::Pretty
    };

    let token = matches
        .get_many::<String>("token")
        .map(|vals| vals.cloned().collect::<Vec<_>>())
        .unwrap_or_default()
        .join(" ")
        .replace("Bearer", "")
        .replace([' ', '\n', '\r', '\t'], "")
        .trim()
        .to_string();

    JwtArgs {
        token,
        print,
        claim_to_clipboard,
    }
}

/// Validates JWT command-line arguments.
///
/// Ensures token is not empty, exits with error code 1 if validation fails.
///
/// # Parameters
/// - `args`: A reference to a `JwtArgs` instance containing the CLI arguments to validate.
///
/// # Behavior
/// - If `args.token` is empty:
///   - Prints an error message: "The token is required."
///   - Exits the program with a status code of `1`.
pub fn validate_cli_arguments(args: &JwtArgs) {
    if args.token.is_empty() {
        eprintln!("The token is required.");
        std::process::exit(1);
    }
}
