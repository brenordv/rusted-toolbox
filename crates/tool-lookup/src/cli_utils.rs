use crate::models::LookupConfig;
use anyhow::Result;
use clap::{Arg, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::constants::general::DASH_LINE;
use shared::system::get_current_working_dir::get_current_working_dir_str;

pub fn print_runtime_info(args: &LookupConfig) {
    println!("Lookup v{}", env!("CARGO_PKG_VERSION"));
    println!("{}", DASH_LINE);

    println!("Text: {}", args.text);
    println!("Path: {}", args.path);
    println!("File extensions: {:?}", args.file_extensions);
    println!("Print Header: {}", !args.no_header);
    if args.current_only {
        println!("Search Mode: Current folder only")
    } else {
        println!("Search Mode: Recursive")
    }
    println!("Print Line data only: {}", args.line_only);
}

pub fn get_cli_arguments() -> Result<LookupConfig> {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_DESCRIPTION"),
            "This tool can search for text (case insensitive) in files and print the results to the console."
        )
        .arg(Arg::new("text")
            .long("text")
            .short('t')
            .required(true)
            .help("Text to search for."))
        .arg(Arg::new("path")
            .long("path")
            .short('p')
            .help("Where to look for the text. (Default: current directory)"))
        .arg(Arg::new("no-header")
        .long("no-header")
        .short('n')
        .action(clap::ArgAction::SetTrue)
        .help("If true, won't print the header. (Default: false)"))
        .arg(Arg::new("line-only")
            .long("line-only")
            .short('l')
            .action(clap::ArgAction::SetTrue)
            .help("If true, will print only the line content, no file data. (Default: false)"))
        .arg(Arg::new("current-only")
            .long("current-only")
            .short('c')
            .action(clap::ArgAction::SetTrue)
            .help("If true, won't search recursively. (Default: false)"))
        .arg(Arg::new("extension")
            .long("extension")
            .short('e')
            .value_name("EXT")
            .action(clap::ArgAction::Append)
            .required(true)
            .help("File extension to look for. May be specified multiple times."))        
        .get_matches();

    let text = matches
        .get_one::<String>("text")
        .cloned()
        .unwrap_or_default();

    let path = matches
        .get_one::<String>("path")
        .cloned()
        .unwrap_or_else(|| get_current_working_dir_str().unwrap_or_default());

    let file_extensions = matches
        .get_many::<String>("extension")
        .unwrap_or_default()
        .cloned()
        .collect::<Vec<String>>();

    let no_header = matches.get_flag("no-header");

    let current_only = matches.get_flag("current-only");

    let line_only = matches.get_flag("line-only");

    Ok(LookupConfig::new(
        path,
        text,
        file_extensions,
        no_header,
        current_only,
        line_only,
    ))
}
