use crate::models::{FollowMode, TailConfig};
use anyhow::{bail, Result};
use clap::{Args, Parser};
use common_cli::common_tool_args::CommonToolArgsNoVerbose;
use common_cli::tool_exit_helpers::exit_error;
use common_cli::tool_log_level::ToolLogLevel;
use shared_head_tail::count_parser::parse_tail_count;
use shared_head_tail::models::{resolve_count, Count, CountPrefix, HeaderPolicy};
use std::time::Duration;
use tracing::error;

/// Print the last part of files.
///
/// Mimics GNU tail: prints the last 10 lines of each FILE to standard output. With more
/// than one FILE, precedes each with a header giving the file name. With no FILE, or when
/// FILE is -, reads standard input.
#[derive(Parser, Debug)]
#[command(name = "tail", about, long_about, version)]
pub struct CliArgs {
    #[command(flatten)]
    pub args: TailArgs,

    #[command(flatten)]
    pub common: CommonToolArgsNoVerbose,
}

/// The tail-specific command line surface.
#[derive(Args, Debug)]
pub struct TailArgs {
    /// Output the last NUM lines; '+NUM' outputs starting with line NUM
    #[arg(
        short = 'n',
        long = "lines",
        value_name = "[+]NUM",
        value_parser = parse_tail_count,
        allow_hyphen_values = true,
        overrides_with_all = ["lines", "bytes"]
    )]
    pub lines: Option<Count>,

    /// Output the last NUM bytes; '+NUM' outputs starting with byte NUM
    #[arg(
        short = 'c',
        long = "bytes",
        value_name = "[+]NUM",
        value_parser = parse_tail_count,
        allow_hyphen_values = true,
        overrides_with_all = ["bytes", "lines"]
    )]
    pub bytes: Option<Count>,

    /// Output appended data as the file grows; the mode binds only as --follow=name or
    /// --follow=descriptor (a detached word after --follow is a file operand)
    #[arg(
        short = 'f',
        long = "follow",
        value_name = "MODE",
        value_enum,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "descriptor"
    )]
    pub follow: Option<FollowMode>,

    /// Same as --follow=name --retry
    #[arg(short = 'F')]
    pub follow_name_retry: bool,

    /// Keep trying to open a file that cannot be opened
    #[arg(long = "retry")]
    pub retry: bool,

    /// Seconds to sleep between follow-mode checks (floating point accepted)
    #[arg(
        short = 's',
        long = "sleep-interval",
        value_name = "N",
        default_value = "1",
        value_parser = parse_sleep_interval
    )]
    pub sleep_interval: Duration,

    /// Accepted for GNU compatibility; has no effect in this port
    #[arg(long = "max-unchanged-stats", value_name = "N")]
    pub max_unchanged_stats: Option<u64>,

    /// Not supported by this port; rejected with an error
    #[arg(long = "pid", value_name = "PID")]
    pub pid: Option<u32>,

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

/// Parses `-s/--sleep-interval`, refusing values `Duration` cannot represent
/// (negative, NaN, or out of range).
fn parse_sleep_interval(raw: &str) -> Result<Duration, String> {
    let seconds: f64 = raw
        .parse()
        .map_err(|_| format!("invalid sleep interval '{raw}'"))?;
    Duration::try_from_secs_f64(seconds)
        .map_err(|error| format!("invalid sleep interval '{raw}': {error}"))
}

/// Builds the engine configuration from the parsed arguments.
///
/// # Errors
/// Fails when `--pid` is given: this port does not support it.
pub fn build_tail_config(args: &TailArgs) -> Result<TailConfig> {
    if args.pid.is_some() {
        bail!("--pid is not supported by this port of tail");
    }

    let (unit, count) = resolve_count(args.lines, args.bytes, CountPrefix::FromEnd);
    let follow = if args.follow_name_retry {
        Some(FollowMode::Name)
    } else {
        args.follow
    };

    Ok(TailConfig {
        unit,
        count: count.value,
        from_start: count.prefix == CountPrefix::FromStart,
        follow,
        retry: args.retry || args.follow_name_retry,
        sleep_interval: args.sleep_interval,
        max_unchanged_stats: args.max_unchanged_stats,
        headers: HeaderPolicy::from_flags(args.quiet, args.verbose_headers),
        zero_terminated: args.zero_terminated,
        files: args.files.clone(),
    })
}

/// Parses the command line, boots logging (and the optional `--app-header`
/// block), and returns the engine configuration. Boot happens before the
/// configuration is validated so the `--pid` rejection reaches the subscriber.
pub fn initialize() -> TailConfig {
    let cli = CliArgs::parse();

    cli.common.app_boot_up_with_level(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        ToolLogLevel::Warn,
        cli.args.verbose_headers,
        false,
        Some(|| {}),
    );

    match build_tail_config(&cli.args) {
        Ok(config) => config,
        Err(error) => {
            error!("{:#}", error);
            exit_error();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use rstest::rstest;
    use shared_head_tail::models::CountUnit;

    fn config(argv: &[&str]) -> TailConfig {
        let cli = CliArgs::try_parse_from(argv).unwrap();
        build_tail_config(&cli.args).unwrap()
    }

    #[test]
    fn cli_definition_has_no_conflicting_flags() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn posix_minus_spelling_counts_from_the_end() {
        let cfg = config(&["tail", "-n", "-5"]);
        assert!(!cfg.from_start);
        assert_eq!(cfg.count, 5);
        assert_eq!(cfg.unit, CountUnit::Lines);
    }

    #[test]
    fn last_count_flag_wins() {
        let cfg = config(&["tail", "-c", "7", "-c", "9"]);
        assert_eq!(cfg.unit, CountUnit::Bytes);
        assert_eq!(cfg.count, 9);

        let cfg = config(&["tail", "-n", "5", "-c", "3"]);
        assert_eq!(cfg.unit, CountUnit::Bytes);
        assert_eq!(cfg.count, 3);
    }

    #[test]
    fn default_count_is_ten_lines() {
        let cfg = config(&["tail"]);
        assert_eq!(cfg.unit, CountUnit::Lines);
        assert_eq!(cfg.count, 10);
        assert!(!cfg.from_start);
        assert_eq!(cfg.follow, None);
    }

    #[test]
    fn bare_follow_means_descriptor_and_operands_stay_operands() {
        let cfg = config(&["tail", "-f", "app.log"]);
        assert_eq!(cfg.follow, Some(FollowMode::Descriptor));
        assert_eq!(cfg.files, vec!["app.log".to_string()]);

        let cfg = config(&["tail", "--follow", "name", "f.txt"]);
        assert_eq!(cfg.follow, Some(FollowMode::Descriptor));
        assert_eq!(cfg.files, vec!["name".to_string(), "f.txt".to_string()]);

        let cfg = config(&["tail", "--follow=name", "f.txt"]);
        assert_eq!(cfg.follow, Some(FollowMode::Name));
        assert_eq!(cfg.files, vec!["f.txt".to_string()]);
    }

    #[test]
    fn big_f_means_follow_name_with_retry() {
        let cfg = config(&["tail", "-F", "f.txt"]);
        assert_eq!(cfg.follow, Some(FollowMode::Name));
        assert!(cfg.retry);
    }

    #[rstest]
    #[case("NaN")]
    #[case("-1")]
    #[case("1e300")]
    #[case("abc")]
    fn sleep_interval_rejects_unrepresentable_values(#[case] raw: &str) {
        assert!(parse_sleep_interval(raw).is_err(), "{raw} should fail");
    }

    #[test]
    fn sleep_interval_defaults_to_one_second() {
        let cfg = config(&["tail"]);
        assert_eq!(cfg.sleep_interval, Duration::from_secs(1));

        let cfg = config(&["tail", "-s", "0.25"]);
        assert_eq!(cfg.sleep_interval, Duration::from_millis(250));
    }

    #[test]
    fn plus_zero_is_accepted_as_from_start() {
        let cfg = config(&["tail", "-n", "+0"]);
        assert!(cfg.from_start);
        assert_eq!(cfg.count, 0);
    }

    #[test]
    fn pid_is_rejected_with_the_unsupported_message() {
        let cli = CliArgs::try_parse_from(["tail", "--pid", "123", "f.txt"]).unwrap();
        let error = build_tail_config(&cli.args).unwrap_err();
        assert!(error.to_string().contains("not supported"));
    }

    #[test]
    fn max_unchanged_stats_is_accepted_and_carried() {
        let cfg = config(&["tail", "--max-unchanged-stats", "5", "f.txt"]);
        assert_eq!(cfg.max_unchanged_stats, Some(5));
    }

    #[test]
    fn quiet_and_verbose_are_last_wins_in_both_spellings() {
        let cfg = config(&["tail", "-q", "-v"]);
        assert_eq!(cfg.headers, HeaderPolicy::Always);

        let cfg = config(&["tail", "-v", "-q"]);
        assert_eq!(cfg.headers, HeaderPolicy::Never);

        let cfg = config(&["tail", "-q", "--verbose"]);
        assert_eq!(cfg.headers, HeaderPolicy::Always);

        let cfg = config(&["tail", "--verbose", "-q"]);
        assert_eq!(cfg.headers, HeaderPolicy::Never);
    }
}
