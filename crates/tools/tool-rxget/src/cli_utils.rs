use crate::models::{RunMode, RxgetConfig};
use crate::targets::expand_targets;
use clap::{Parser, ValueEnum};
use common_cli::common_tool_args::CommonToolArgs;
use common_cli::header_format::format_config_item;
use common_cli::tool_exit_helpers::exit_error;
use regex::bytes::Regex;
use tracing::error;

/// Extract regex-matched values from text files.
///
/// Applies the pattern to every line of every target and prints the extracted values, one per
/// line. The value is capture group 1 when the pattern has capture groups, the whole match
/// otherwise. Targets are literal file paths and/or wildcard patterns; the tool expands
/// wildcards itself, so patterns work the same in shells that do not (Windows).
#[derive(Parser, Debug)]
#[command(about, long_about, version)]
pub struct CliArgs {
    /// Files and/or wildcard patterns to scan
    #[arg(value_name = "TARGET", required = true, num_args = 1..)]
    pub targets: Vec<String>,

    /// Extraction regex; group 1 is the value when the pattern has capture groups
    #[arg(short = 'p', long = "pattern", value_name = "REGEX", required = true)]
    pub pattern: String,

    /// What to print: every occurrence, or unique values per file or per run
    #[arg(
        short = 'm',
        long = "mode",
        value_name = "MODE",
        value_enum,
        default_value_t = RunMode::All
    )]
    pub mode: RunMode,

    /// Prefix each value with the file it came from, as `<filename>: <value>`
    #[arg(short = 'H', long = "with-filename")]
    pub with_filename: bool,

    #[command(flatten)]
    pub common: CommonToolArgs,
}

/// Displays the runtime configuration, shown under `--app-header`.
///
/// Reports the raw arguments as given; the resolved target list does not
/// exist yet at boot time (the debug-level run summary carries its count).
fn print_runtime_info(args: &CliArgs) {
    let mode = args
        .mode
        .to_possible_value()
        .map(|value| value.get_name().to_string())
        .unwrap_or_else(|| format!("{:?}", args.mode));
    println!("{}", format_config_item("Pattern", &args.pattern));
    println!("{}", format_config_item("Mode", mode));
    println!(
        "{}",
        format_config_item("With filename", args.with_filename)
    );
    println!(
        "{}",
        format_config_item("Targets", format!("{:?}", args.targets))
    );
}

/// Parses the command line and builds the engine configuration.
///
/// Boot order is fixed: parse, initialize logging (so every later diagnostic
/// reaches the subscriber), compile the regex, expand the targets. An invalid
/// pattern or an empty expansion exits 1 before any file is opened. The
/// returned flag says whether any target argument failed to resolve; the run
/// must end with the failure exit even when extraction succeeds elsewhere.
pub fn initialize() -> (RxgetConfig, bool) {
    let args = CliArgs::parse();

    args.common.app_boot_up(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        false,
        false,
        Some(|| {
            print_runtime_info(&args);
        }),
    );

    let pattern = match Regex::new(&args.pattern) {
        Ok(pattern) => pattern,
        Err(compile_error) => {
            error!("invalid pattern: {compile_error}");
            exit_error();
        }
    };

    let expansion = expand_targets(&args.targets);
    if expansion.targets.is_empty() {
        error!("no input files remain after resolving the targets");
        exit_error();
    }

    let config = RxgetConfig {
        pattern,
        mode: args.mode,
        with_filename: args.with_filename,
        targets: expansion.targets,
    };
    (config, expansion.failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_has_no_conflicting_flags() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn mode_defaults_to_all_and_parses_the_kebab_names() {
        let args = CliArgs::try_parse_from(["rxget", "-p", "x", "file.txt"]).unwrap();
        assert_eq!(args.mode, RunMode::All);

        let args =
            CliArgs::try_parse_from(["rxget", "-p", "x", "-m", "unique-per-file", "file.txt"])
                .unwrap();
        assert_eq!(args.mode, RunMode::UniquePerFile);

        let args =
            CliArgs::try_parse_from(["rxget", "-p", "x", "--mode", "unique-per-run", "file.txt"])
                .unwrap();
        assert_eq!(args.mode, RunMode::UniquePerRun);
    }

    #[test]
    fn pattern_and_at_least_one_target_are_required() {
        assert!(CliArgs::try_parse_from(["rxget", "file.txt"]).is_err());
        assert!(CliArgs::try_parse_from(["rxget", "-p", "x"]).is_err());
    }

    #[test]
    fn print_runtime_info_smoke() {
        let args =
            CliArgs::try_parse_from(["rxget", "-p", r"id=(\d+)", "-H", "a.txt", "*.log"]).unwrap();
        print_runtime_info(&args);
    }
}
