use crate::models::DistroCcRuntimeConfig;
use anyhow::Result;
use clap::{Arg, ArgAction, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;
use std::env;

/// Displays runtime configuration information.
pub fn print_runtime_info(from: &str, to: &str, command: &str, no_header: bool, verbose: bool) {
    println!("Distro-cc v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);
    println!("- From: {}", from);
    println!("- To: {}", to);
    println!("- Command: {}", command);
    println!("- No header: {}", no_header);
    println!("- Verbose: {}", verbose);
    println!(
        "Warning: package managers are not fully equivalent; flags and package names can differ.\n\n"
    );
}

/// Parses command-line arguments into Distro-cc configuration.
///
/// # Errors
/// Returns error if arguments are invalid or missing.
pub fn get_cli_arguments() -> Result<DistroCcRuntimeConfig> {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_DESCRIPTION"),
            "Translate package manager commands between Linux distributions.",
        )
        .arg(
            Arg::new("from")
                .long("from")
                .short('f')
                .value_name("DISTRO")
                .required(true)
                .help("Name of the distro the current command is in"),
        )
        .arg(
            Arg::new("to")
                .long("to")
                .short('t')
                .value_name("DISTRO")
                .help("Target distro to convert the command to (optional)"),
        )
        .arg(
            Arg::new("command")
                .long("command")
                .short('c')
                .value_name("COMMAND")
                .num_args(1..)
                .trailing_var_arg(true)
                .action(ArgAction::Append)
                .required(true)
                .help("Command to be converted"),
        )
        .arg(
            Arg::new("no-header")
                .long("no-header")
                .short('n')
                .action(ArgAction::SetTrue)
                .help("If true, suppresses the header output"),
        )
        .arg(
            Arg::new("verbose")
                .long("verbose")
                .short('v')
                .action(ArgAction::SetTrue)
                .help("If true, logs what the app is doing"),
        )
        .get_matches();

    let from = matches
        .get_one::<String>("from")
        .cloned()
        .unwrap_or_default();

    let to = matches.get_one::<String>("to").cloned();

    let command_parts = matches
        .get_many::<String>("command")
        .unwrap_or_default()
        .cloned()
        .collect::<Vec<String>>();

    if command_parts.is_empty() {
        return Err(anyhow::anyhow!(
            "No command provided. Use --command to pass the command to convert."
        ));
    }

    let command = command_parts.join(" ");
    let no_header = matches.get_flag("no-header");
    let verbose = matches.get_flag("verbose");

    Ok(DistroCcRuntimeConfig::new(
        from, to, command, no_header, verbose,
    ))
}
