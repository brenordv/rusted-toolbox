use crate::models::HeadConfig;
use clap::{Args, Parser};
use common_cli::common_tool_args::CommonToolArgsNoVerbose;
use common_cli::tool_log_level::ToolLogLevel;
use shared_head_tail::count_parser::parse_head_count;
use shared_head_tail::models::{resolve_count, Count, CountPrefix, HeaderPolicy};

/// Print the first part of files.
///
/// Mimics GNU head: prints the first 10 lines of each FILE to standard output. With more
/// than one FILE, precedes each with a header giving the file name. With no FILE, or when
/// FILE is -, reads standard input.
#[derive(Parser, Debug)]
#[command(name = "head", about, long_about, version)]
pub struct CliArgs {
    #[command(flatten)]
    pub args: HeadArgs,

    #[command(flatten)]
    pub common: CommonToolArgsNoVerbose,
}

/// The head-specific command line surface.
#[derive(Args, Debug)]
pub struct HeadArgs {
    /// Print the first NUM lines; with a leading '-', print all but the last NUM lines
    #[arg(
        short = 'n',
        long = "lines",
        value_name = "[-]NUM",
        value_parser = parse_head_count,
        allow_hyphen_values = true,
        overrides_with_all = ["lines", "bytes"]
    )]
    pub lines: Option<Count>,

    /// Print the first NUM bytes; with a leading '-', print all but the last NUM bytes
    #[arg(
        short = 'c',
        long = "bytes",
        value_name = "[-]NUM",
        value_parser = parse_head_count,
        allow_hyphen_values = true,
        overrides_with_all = ["bytes", "lines"]
    )]
    pub bytes: Option<Count>,

    /// Never print headers giving file names
    #[arg(
        short = 'q',
        long = "quiet",
        visible_alias = "silent",
        overrides_with_all = ["quiet", "verbose_headers"]
    )]
    pub quiet: bool,

    /// Always print headers giving file names
    #[arg(short = 'v', long = "verbose", overrides_with = "verbose_headers")]
    pub verbose_headers: bool,

    /// Line delimiter is NUL, not newline
    #[arg(short = 'z', long = "zero-terminated")]
    pub zero_terminated: bool,

    /// Files to read; with none (or '-') reads standard input
    #[arg(value_name = "FILE", num_args = 0..)]
    pub files: Vec<String>,
}

/// Builds the engine configuration from the parsed arguments.
pub fn build_head_config(args: &HeadArgs) -> HeadConfig {
    let (unit, count) = resolve_count(args.lines, args.bytes, CountPrefix::Plain);
    HeadConfig {
        unit,
        count: count.value,
        elide: count.prefix == CountPrefix::FromEnd,
        headers: HeaderPolicy::from_flags(args.quiet, args.verbose_headers),
        zero_terminated: args.zero_terminated,
        files: args.files.clone(),
    }
}

/// Parses the command line, boots logging (and the optional `--app-header`
/// block), and returns the engine configuration.
pub fn initialize() -> HeadConfig {
    let cli = CliArgs::parse();

    cli.common.app_boot_up_with_level(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        ToolLogLevel::Warn,
        cli.args.verbose_headers,
        false,
        Some(|| {}),
    );

    build_head_config(&cli.args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use shared_head_tail::models::CountUnit;

    fn config(argv: &[&str]) -> HeadConfig {
        let cli = CliArgs::try_parse_from(argv).unwrap();
        build_head_config(&cli.args)
    }

    #[test]
    fn cli_definition_has_no_conflicting_flags() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn hyphen_leading_counts_parse_as_elide() {
        let cfg = config(&["head", "-n", "-2"]);
        assert!(cfg.elide);
        assert_eq!(cfg.count, 2);
        assert_eq!(cfg.unit, CountUnit::Lines);

        let cfg = config(&["head", "-n", "-2K"]);
        assert!(cfg.elide);
        assert_eq!(cfg.count, 2048);

        let cfg = config(&["head", "-c", "-4"]);
        assert!(cfg.elide);
        assert_eq!(cfg.unit, CountUnit::Bytes);
        assert_eq!(cfg.count, 4);
    }

    #[test]
    fn last_count_flag_wins() {
        let cfg = config(&["head", "-n", "5", "-c", "3"]);
        assert_eq!(cfg.unit, CountUnit::Bytes);
        assert_eq!(cfg.count, 3);

        let cfg = config(&["head", "-c", "3", "-n", "5"]);
        assert_eq!(cfg.unit, CountUnit::Lines);
        assert_eq!(cfg.count, 5);

        let cfg = config(&["head", "-n", "5", "-n", "2"]);
        assert_eq!(cfg.unit, CountUnit::Lines);
        assert_eq!(cfg.count, 2);
    }

    #[test]
    fn default_count_is_ten_lines() {
        let cfg = config(&["head"]);
        assert_eq!(cfg.unit, CountUnit::Lines);
        assert_eq!(cfg.count, 10);
        assert!(!cfg.elide);
    }

    #[test]
    fn quiet_and_verbose_are_last_wins_in_both_spellings() {
        let cfg = config(&["head", "-q", "-v"]);
        assert_eq!(cfg.headers, HeaderPolicy::Always);

        let cfg = config(&["head", "-v", "-q"]);
        assert_eq!(cfg.headers, HeaderPolicy::Never);

        let cfg = config(&["head", "-q", "--verbose"]);
        assert_eq!(cfg.headers, HeaderPolicy::Always);

        let cfg = config(&["head", "--verbose", "-q"]);
        assert_eq!(cfg.headers, HeaderPolicy::Never);

        let cfg = config(&["head", "--silent", "--verbose"]);
        assert_eq!(cfg.headers, HeaderPolicy::Always);
    }

    #[test]
    fn plus_count_is_rejected_at_parse_time() {
        assert!(CliArgs::try_parse_from(["head", "-n", "+5"]).is_err());
    }
}
