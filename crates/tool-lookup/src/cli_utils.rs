use crate::models::{FilesLookupConfig, LookupCommand, PatternMode, TextLookupConfig};
use anyhow::Result;
use clap::{Arg, ArgAction, ArgGroup, Command};
use shared::command_line::cli_builder::CommandExt;
use shared::system::get_current_working_dir::get_current_working_dir_str;

pub fn get_cli_arguments() -> Result<LookupCommand> {
    let text_cmd = Command::new("text")
        .about("Search for text (case-insensitive) inside files")
        .arg(
            Arg::new("TEXT")
                .help("Text to search for")
                .required_unless_present("text")
                .index(1),
        )
        .arg(
            Arg::new("text")
                .long("text")
                .short('t')
                .help("Text to search for (alternative to positional)")
                .conflicts_with("TEXT"),
        )
        .arg(
            Arg::new("path")
                .long("path")
                .short('p')
                .help("Where to look for the text. (Default: current directory)"),
        )
        .arg(
            Arg::new("no-header")
                .long("no-header")
                .short('n')
                .action(ArgAction::SetTrue)
                .help("If true, won't print the header. (Default: false)"),
        )
        .arg(
            Arg::new("line-only")
                .long("line-only")
                .short('l')
                .action(ArgAction::SetTrue)
                .help("If true, will print only the line content, no file data. (Default: false)"),
        )
        .arg(
            Arg::new("current-only")
                .long("current-only")
                .short('c')
                .action(ArgAction::SetTrue)
                .help("If true, won't search recursively. (Default: false)"),
        )
        .arg(
            Arg::new("extension")
                .long("extension")
                .short('e')
                .value_name("EXT")
                .action(ArgAction::Append)
                .required(true)
                .help("File extension to look for. May be specified multiple times."),
        )
        .group(ArgGroup::new("text_input").args(["TEXT", "text"]))
        ;

    let files_cmd = Command::new("files")
        .about("Search for files by name (case-insensitive by default)")
        .after_help("Examples:\n  lookup files \"*.rs\"\n  lookup files --regex \"^mydoc\\.(pdf|epub|mobi)$\"")
        .arg(
            Arg::new("PATTERN")
                .help("Filename pattern(s) to match. Supports wildcard or regex.")
                .required(true)
                .num_args(1..)
                .index(1),
        )
        .arg(
            Arg::new("path")
                .long("path")
                .short('p')
                .help("Where to search. (Default: current directory)"),
        )
        .arg(
            Arg::new("regex")
                .long("regex")
                .action(ArgAction::SetTrue)
                .help("Use regex pattern matching (default: wildcard)"),
        )
        .arg(
            Arg::new("wildcard")
                .long("wildcard")
                .action(ArgAction::SetTrue)
                .help("Use wildcard/glob pattern matching (default)"),
        )
        .arg(
            Arg::new("case-sensitive")
                .long("case-sensitive")
                .action(ArgAction::SetTrue)
                .help("Make pattern matching case-sensitive (default: insensitive)"),
        )
        .arg(
            Arg::new("current-only")
                .long("current-only")
                .short('c')
                .action(ArgAction::SetTrue)
                .help("If set, won't search recursively (default: recursive)"),
        )
        .arg(
            Arg::new("no-header")
                .long("no-header")
                .short('n')
                .action(ArgAction::SetTrue)
                .help("Suppress header output"),
        )
        .arg(
            Arg::new("no-progress")
                .long("no-progress")
                .action(ArgAction::SetTrue)
                .help("Suppress progress (current folder) updates"),
        )
        .arg(
            Arg::new("no-errors")
                .long("no-errors")
                .action(ArgAction::SetTrue)
                .help("Suppress error messages during traversal"),
        )
        .arg(
            Arg::new("no-summary")
                .long("no-summary")
                .action(ArgAction::SetTrue)
                .help("Suppress final summary output"),
        )
        .group(ArgGroup::new("pattern-mode").args(["regex", "wildcard"]))
        ;

    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .add_basic_metadata(
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_DESCRIPTION"),
            "Search for text in files or find files by name."
        )
        .subcommand(text_cmd)
        .subcommand(files_cmd)
        .subcommand_required(true)
        .arg_required_else_help(true)
        .get_matches();

    match matches.subcommand() {
        Some(("text", sub_m)) => {
            let text = sub_m
                .get_one::<String>("TEXT")
                .cloned()
                .or_else(|| sub_m.get_one::<String>("text").cloned())
                .unwrap_or_default();

            let path = sub_m
                .get_one::<String>("path")
                .cloned()
                .unwrap_or_else(|| get_current_working_dir_str().unwrap_or_default());

            let file_extensions = sub_m
                .get_many::<String>("extension")
                .unwrap_or_default()
                .cloned()
                .collect::<Vec<String>>();

            let no_header = sub_m.get_flag("no-header");
            let current_only = sub_m.get_flag("current-only");
            let line_only = sub_m.get_flag("line-only");

            Ok(LookupCommand::Text(TextLookupConfig::new(
                path,
                text,
                file_extensions,
                no_header,
                current_only,
                line_only,
            )))
        }
        Some(("files", sub_m)) => {
            let patterns = sub_m
                .get_many::<String>("PATTERN")
                .unwrap_or_default()
                .cloned()
                .collect::<Vec<_>>();

            let path = sub_m
                .get_one::<String>("path")
                .cloned()
                .unwrap_or_else(|| get_current_working_dir_str().unwrap_or_default());

            let pattern_mode = if sub_m.get_flag("regex") {
                PatternMode::Regex
            } else {
                PatternMode::Wildcard
            };

            let case_sensitive = sub_m.get_flag("case-sensitive");
            let recursive = !sub_m.get_flag("current-only");
            let no_header = sub_m.get_flag("no-header");
            let no_progress = sub_m.get_flag("no-progress");
            let no_errors = sub_m.get_flag("no-errors");
            let no_summary = sub_m.get_flag("no-summary");

            Ok(LookupCommand::Files(FilesLookupConfig::new(
                path,
                patterns,
                pattern_mode,
                case_sensitive,
                recursive,
                no_header,
                no_progress,
                no_errors,
                no_summary,
            )))
        }
        _ => {
            // Default to help if no subcommand provided
            // Emulate `text` subcommand with help by returning an error
            // but clap already handles it; we can fallback to empty
            // For robustness, default to `text` with empty text
            let path = get_current_working_dir_str().unwrap_or_default();
            Ok(LookupCommand::Text(TextLookupConfig::new(
                path,
                String::new(),
                vec![],
                false,
                false,
                false,
            )))
        }
    }
}