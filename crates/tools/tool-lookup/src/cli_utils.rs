use crate::models::{FilesLookupConfig, LookupCommand, PatternMode, TextLookupConfig};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::tool_exit_helpers::exit_error;
use common_utils::constants::CONFIG_UL_ITEM_LEVEL_2;
use std::path::PathBuf;
use tracing::error;

/// Search for text (case-insensitive) inside files
///
/// A CLI utility for searching text snippets within files (`text`) or finding files by name using
/// wildcard or regex patterns (`files`), with case-insensitive matching by default, recursive or
/// current-folder-only scanning, configurable output, and per-subcommand headers.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
pub struct CliArgs {
    #[command(subcommand)]
    command: Commands,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

#[derive(Args, Debug)]
struct TextArgs {
    /// Text to search for
    #[arg(
        id = "TEXT",
        value_name = "TEXT",
        required_unless_present = "text",
        group = "text_input"
    )]
    pub text_positional: Option<String>,

    /// Where to look for the text.
    #[arg(short = 'p', long = "path", required = false, default_value = ".")]
    pub path: PathBuf,

    /// If true, will print only the line content, no file data.
    #[arg(short = 'l', long = "line-only", required = false)]
    pub line_only: bool,

    /// If true, won't search recursively.
    #[arg(short = 'c', long = "current-only", required = false)]
    pub current_only: bool,

    /// File extension to look for. May be specified multiple times. If omitted, will search all file extensions.
    #[arg(short = 'e', long = "extension", value_name = "EXT", required = false)]
    pub file_extensions: Vec<String>,

    /// Suppress final summary output
    #[arg(short = 'm', long = "no-summary", required = false)]
    no_summary: bool,
}

#[derive(Args, Debug)]
struct FilesArgs {
    /// Filename pattern(s) to match. Supports wildcard or regex.
    #[arg(
        value_name = "PATTERN",
        required = true,
        num_args = 1..
    )]
    pub patterns: Vec<String>,

    /// Where to look for files.
    #[arg(short = 'p', long = "path", required = false, default_value = ".")]
    pub path: PathBuf,

    /// Pattern that will be used to match filenames.
    #[arg(short='s', long="file-search-pattern", required=false, default_value_t=PatternMode::Wildcard)]
    pub file_search_pattern: PatternMode,

    /// Make pattern matching case-sensitive. (Note: Regex patterns can influence this option.)
    #[arg(short = 'c', long = "case-sensitive", required = false)]
    case_sensitive: bool,

    /// If set, won't search recursively (default: recursive)
    #[arg(short = 'n', long = "no-recursive", required = false)]
    no_recursive: bool,

    /// Suppress progress updates
    #[arg(short = 'o', long = "no-progress", required = false)]
    no_progress: bool,

    /// Suppress error messages during traversal
    #[arg(short = 'e', long = "no-errors", required = false)]
    no_errors: bool,

    /// Suppress final summary output
    #[arg(short = 'm', long = "no-summary", required = false)]
    no_summary: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    //TODO: Bring the examples from the readme file to the after_help
    /// Search for text (case-insensitive) inside a file
    #[command(after_help = "Examples:\n  lookup text \"todo\" --current-only -e rs -e .md")]
    Text(TextArgs),

    //TODO: Bring the examples from the readme file to the after_help
    /// Search for files by name (case-insensitive by default)
    #[command(
        after_help = "Examples:\n  lookup files \"*.rs\"\n  lookup files --regex \"^mydoc\\.(pdf|epub|mobi)$\""
    )]
    Files(FilesArgs),
}

pub fn initialize() -> Result<LookupCommand> {
    let args = CliArgs::parse();

    let command_config = match args.command {
        Commands::Text(txt_cmd_args) => {
            let path = txt_cmd_args.path;
            let text = txt_cmd_args.text_positional.unwrap_or_default();
            let file_extensions = txt_cmd_args.file_extensions;
            let current_only = txt_cmd_args.current_only;
            let line_only = txt_cmd_args.line_only;
            let no_summary = txt_cmd_args.no_summary;
            LookupCommand::Text(TextLookupConfig::new(
                path,
                text,
                file_extensions,
                current_only,
                line_only,
                no_summary,
            ))
        }
        Commands::Files(file_cmd_args) => {
            let path = file_cmd_args.path;
            let patterns = file_cmd_args.patterns;
            let pattern_mode = file_cmd_args.file_search_pattern;
            let case_sensitive = file_cmd_args.case_sensitive;
            let no_recursive = file_cmd_args.no_recursive;
            let no_progress = file_cmd_args.no_progress;
            let no_errors = file_cmd_args.no_errors;
            let no_summary = file_cmd_args.no_summary;
            LookupCommand::Files(FilesLookupConfig::new(
                path,
                patterns,
                pattern_mode,
                case_sensitive,
                no_recursive,
                no_progress,
                no_errors,
                no_summary,
            ))
        }
    };

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| match &command_config {
            LookupCommand::Text(txt_args) => print_txt_header(txt_args),
            LookupCommand::Files(file_args) => print_files_header(file_args),
        }),
    );

    match &command_config {
        LookupCommand::Text(txt_args) => {
            if !txt_args.path.exists() {
                error!(
                    "Path does not exist (or it's not accessible): {:?}",
                    txt_args.path
                );
                exit_error();
            }

            if txt_args.text.is_empty() {
                error!("Text cannot be empty. We need something to search for.");
                exit_error();
            }
        }
        LookupCommand::Files(file_args) => {
            if !file_args.path.exists() {
                error!(
                    "Path does not exist (or it's not accessible): {:?}",
                    file_args.path
                );
                exit_error();
            }

            if !file_args.path.is_dir() {
                error!("Path is not a directory: {:?}", file_args.path);
                exit_error();
            }

            if file_args.patterns.is_empty() {
                error!("Patterns cannot be empty. We need something to search for.");
                exit_error();
            }
        }
    };

    Ok(command_config)
}

pub fn print_txt_header(args: &TextLookupConfig) {
    println!("{} Text: {}", CONFIG_UL_ITEM_LEVEL_2, args.text);
    println!("{} Path: {:?}", CONFIG_UL_ITEM_LEVEL_2, args.path);
    println!(
        "{} File extensions: {:?}",
        CONFIG_UL_ITEM_LEVEL_2, args.file_extensions
    );
    if args.current_only {
        println!(
            "{} Search Mode: Current folder only",
            CONFIG_UL_ITEM_LEVEL_2
        )
    } else {
        println!("{} Search Mode: Recursive", CONFIG_UL_ITEM_LEVEL_2)
    }
    println!("Print Line data only: {}", args.line_only);
}

pub fn print_files_header(args: &FilesLookupConfig) {
    println!("{} Mode: files (by filename)", CONFIG_UL_ITEM_LEVEL_2);
    println!("{} Path: {:?}", CONFIG_UL_ITEM_LEVEL_2, args.path);
    println!("{} Patterns: {:?}", CONFIG_UL_ITEM_LEVEL_2, args.patterns);
    println!(
        "{} Pattern type: {} | Case-sensitive: {} | Current folder only: {}",
        CONFIG_UL_ITEM_LEVEL_2,
        match args.pattern_mode {
            PatternMode::Wildcard => "wildcard",
            PatternMode::Regex => "regex",
        },
        args.case_sensitive,
        args.no_recursive
    );
}
