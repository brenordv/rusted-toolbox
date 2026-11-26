use crate::models::{DepCheckArgs, ExitCode, Format};
use clap::Parser;
use clap::error::ErrorKind;
use common_cli::common_tool_args::CommonToolArgsNoVerbose;
use common_cli::tool_log_level::ToolLogLevel;
use common_utils::string_utils::display_lines;
use dep_check_core::ecosystem::Ecosystem;
use std::ffi::OsString;

/// The tool and its version, as the report names it.
pub const TOOL: &str = concat!("dep-check ", env!("CARGO_PKG_VERSION"));
/// The `User-Agent` of every request.
pub const USER_AGENT: &str = concat!(
    "dep-check/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/brenordv/rusted-toolbox)"
);

/// Checks packages of one ecosystem for known vulnerabilities.
///
/// Asks OSV.dev and deps.dev about each package and prints one merged report. A package
/// without a version is checked at its latest version. With no PACKAGE, the packages are read
/// from stdin, one per line. No flag changes which services are asked or where.
///
/// Exit codes: 0 nothing found, 1 vulnerabilities found, 2 usage or input error, 3 some
/// packages could not be checked or are not known, 4 no service answered.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
pub struct CliArgs {
    /// Packages as name or name@version; read from stdin, one per line, when none is given
    pub packages: Vec<String>,

    /// The packages' ecosystem: cargo, nuget, pypi, npm, rubygems (also rust, crates,
    /// crates.io, dotnet, csharp, c#, python, pip, node, javascript, js, typescript, ts, ruby,
    /// gem)
    #[arg(short, long, value_name = "ECOSYSTEM", value_parser = parse_ecosystem)]
    pub ecosystem: Ecosystem,

    /// Output format
    #[arg(short, long, value_enum, default_value_t = Format::Text)]
    pub format: Format,

    #[command(flatten)]
    pub common: CommonToolArgsNoVerbose,
}

impl CliArgs {
    /// The parsed values without the logging flags.
    pub fn into_args(self) -> DepCheckArgs {
        DepCheckArgs {
            ecosystem: self.ecosystem,
            format: self.format,
            packages: self.packages,
        }
    }
}

fn parse_ecosystem(text: &str) -> Result<Ecosystem, String> {
    text.parse::<Ecosystem>()
}

/// Refuses the flags that would put something other than the report on stdout when it holds
/// JSON: log lines (`--log-to-console`) and the header block (`--app-header`).
///
/// # Errors
/// The line to show, naming the flag.
pub fn check_stdout(args: &CliArgs) -> Result<(), String> {
    if args.format != Format::Json {
        return Ok(());
    }
    if args.common.log_to_stdout {
        return Err(
            "--log-to-console writes log lines to stdout, which holds the JSON report; leave it out, or use --log-to-file".to_owned(),
        );
    }
    if args.common.app_header {
        return Err(
            "--app-header prints to stdout, which holds the JSON report; leave it out with --format json".to_owned(),
        );
    }
    Ok(())
}

/// What to print, and where, when the command line ends the run before a check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EarlyExit {
    pub text: String,
    /// Help and the version go to stdout; everything else to stderr.
    pub to_stdout: bool,
    pub code: i32,
}

impl EarlyExit {
    fn usage(text: String) -> Self {
        Self {
            text,
            to_stdout: false,
            code: ExitCode::Usage.code(),
        }
    }
}

/// The refusal for `--help` or `--version` among other arguments.
pub const HELP_STANDS_ALONE: &str = "--help and --version must stand alone; to check a package list that may hold a dash name, put the packages after --";

/// Parses `argv` (the program name first) without exiting.
///
/// `--help` and `--version` are honored only as the one argument: in a package list built by a
/// script they would otherwise print and exit 0 with nothing checked. clap's error text is put
/// on clean lines, since it quotes the argument it refused.
///
/// # Errors
/// What to print and the code to exit with, when the run ends here.
pub fn parse_command_line<I, T>(argv: I) -> Result<CliArgs, EarlyExit>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let argv: Vec<OsString> = argv.into_iter().map(Into::into).collect();
    let error = match CliArgs::try_parse_from(&argv) {
        Ok(args) => {
            check_stdout(&args).map_err(EarlyExit::usage)?;
            return Ok(args);
        }
        Err(error) => error,
    };
    let text = display_lines(&error.to_string());
    match error.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion if argv.len() == 2 => Err(EarlyExit {
            text,
            to_stdout: true,
            code: error.exit_code(),
        }),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            Err(EarlyExit::usage(HELP_STANDS_ALONE.to_owned()))
        }
        // clap's ErrorKind is non-exhaustive; every other kind is a usage error clap words itself.
        _ => Err(EarlyExit {
            text,
            to_stdout: false,
            code: error.exit_code(),
        }),
    }
}

/// Parses the command line, refuses flags that would mix with a JSON report, and boots logging
/// at the house default: warnings and errors on stderr.
///
/// # Errors
/// What to print and the code to exit with when the run ends before a check; logging is not
/// booted then.
pub fn initialize() -> Result<DepCheckArgs, EarlyExit> {
    let args = parse_command_line(std::env::args_os())?;
    args.common.app_boot_up_with_level(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        ToolLogLevel::Warn,
        "<unused>",
        false,
        None::<fn()>,
    );
    Ok(args.into_args())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use dep_check_core::ecosystem::ALIASES;
    use rstest::rstest;

    fn ecosystem_help_words() -> Vec<String> {
        let command = CliArgs::command();
        let ecosystem = command
            .get_arguments()
            .find(|argument| argument.get_id() == "ecosystem")
            .unwrap();
        ecosystem
            .get_help()
            .unwrap()
            .to_string()
            .split(|c: char| c.is_whitespace() || matches!(c, ',' | '(' | ')' | ':'))
            .filter(|word| !word.is_empty())
            .map(str::to_owned)
            .collect()
    }

    #[rstest]
    #[case::help(&["dep-check", "--help"])]
    #[case::short_help(&["dep-check", "-h"])]
    #[case::version(&["dep-check", "--version"])]
    #[case::short_version(&["dep-check", "-V"])]
    fn help_or_version_alone_prints_to_stdout_and_exits_0(#[case] command_line: &[&str]) {
        let early = parse_command_line(command_line).unwrap_err();

        assert_eq!((early.to_stdout, early.code), (true, 0));
    }

    #[rstest]
    #[case::version_among_packages(&["dep-check", "-e", "cargo", "tokio@1.20.0", "--version"])]
    #[case::help_among_packages(&["dep-check", "-e", "cargo", "-h", "serde"])]
    #[case::help_after_the_ecosystem(&["dep-check", "-e", "cargo", "--help"])]
    fn help_or_version_among_other_arguments_is_a_usage_error(#[case] command_line: &[&str]) {
        let early = parse_command_line(command_line).unwrap_err();

        assert_eq!(
            early,
            EarlyExit {
                text: HELP_STANDS_ALONE.to_owned(),
                to_stdout: false,
                code: 2
            }
        );
    }

    #[test]
    fn a_dash_name_after_the_separator_is_a_package() {
        let args = parse_command_line(["dep-check", "-e", "cargo", "--", "--help"]).unwrap();

        assert_eq!(args.packages, ["--help"]);
    }

    #[test]
    fn a_refused_argument_is_shown_on_clean_lines() {
        let early =
            parse_command_line(["dep-check", "-e", "cargo", "--x\u{1b}]0;owned\u{7}"]).unwrap_err();

        assert_eq!((early.to_stdout, early.code), (false, 2));
        assert!(!early.text.contains('\u{1b}'), "{:?}", early.text);
        assert!(!early.text.contains('\u{7}'), "{:?}", early.text);
    }

    #[test]
    fn a_json_run_with_log_lines_on_stdout_is_a_usage_error() {
        let early = parse_command_line([
            "dep-check",
            "-e",
            "cargo",
            "--format",
            "json",
            "--log-to-console",
        ])
        .unwrap_err();

        assert_eq!((early.to_stdout, early.code), (false, 2));
        assert!(early.text.starts_with("--log-to-console"), "{}", early.text);
    }

    #[test]
    fn cli_definition_has_no_conflicting_flags() {
        CliArgs::command().debug_assert();
    }

    #[rstest]
    #[case::short(&["dep-check", "-e", "cargo", "a", "b"], Ecosystem::Cargo, 2)]
    #[case::long_alias(&["dep-check", "--ecosystem", "rust", "a"], Ecosystem::Cargo, 1)]
    #[case::no_packages(&["dep-check", "-e", "npm"], Ecosystem::Npm, 0)]
    fn ecosystem_and_packages_parse(
        #[case] command_line: &[&str],
        #[case] ecosystem: Ecosystem,
        #[case] packages: usize,
    ) {
        let args = CliArgs::try_parse_from(command_line).unwrap().into_args();

        assert_eq!((args.ecosystem, args.packages.len()), (ecosystem, packages));
    }

    #[test]
    fn every_ecosystem_word_parses_and_is_in_the_help() {
        let help = ecosystem_help_words();

        for (alias, ecosystem) in ALIASES {
            let args = CliArgs::try_parse_from(["dep-check", "-e", alias]).unwrap();
            assert_eq!(args.ecosystem, *ecosystem, "{alias}");
            assert!(
                help.iter().any(|word| word == alias),
                "{alias} is not a word of the --ecosystem help: {help:?}"
            );
        }
    }

    #[test]
    fn a_missing_ecosystem_is_a_usage_error() {
        let error = CliArgs::try_parse_from(["dep-check", "serde"]).unwrap_err();

        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn an_unknown_ecosystem_lists_the_five() {
        let error = CliArgs::try_parse_from(["dep-check", "-e", "cobol", "x"]).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("use one of: cargo, nuget, pypi, npm, rubygems"),
            "{error}"
        );
    }

    #[rstest]
    #[case::default(&["dep-check", "-e", "cargo"], Format::Text)]
    #[case::json(&["dep-check", "-e", "cargo", "--format", "json"], Format::Json)]
    #[case::short(&["dep-check", "-e", "cargo", "-f", "json"], Format::Json)]
    fn the_format_parses_and_defaults_to_text(
        #[case] command_line: &[&str],
        #[case] format: Format,
    ) {
        let args = CliArgs::try_parse_from(command_line).unwrap();

        assert_eq!(args.format, format);
    }

    #[rstest]
    #[case::log_to_console("--log-to-console", "--log-to-console")]
    #[case::app_header("--app-header", "--app-header")]
    fn json_refuses_a_flag_that_writes_to_stdout(#[case] flag: &str, #[case] named: &str) {
        let args = CliArgs::try_parse_from(["dep-check", "-e", "cargo", "--format", "json", flag])
            .unwrap();

        let refused = check_stdout(&args).unwrap_err();

        assert!(refused.starts_with(named), "{refused}");
    }

    #[rstest]
    #[case::log_to_console("--log-to-console")]
    #[case::app_header("--app-header")]
    fn text_takes_the_flags_json_refuses(#[case] flag: &str) {
        let args = CliArgs::try_parse_from(["dep-check", "-e", "cargo", flag]).unwrap();

        assert_eq!(check_stdout(&args), Ok(()));
    }

    #[rstest]
    #[case::osv_url("--osv-url")]
    #[case::base_url("--base-url")]
    #[case::url("--url")]
    #[case::server("--server")]
    fn no_flag_points_a_query_elsewhere(#[case] flag: &str) {
        let result = CliArgs::try_parse_from(["dep-check", "-e", "cargo", flag, "x", "serde"]);

        assert_eq!(result.unwrap_err().kind(), ErrorKind::UnknownArgument);
    }
}
