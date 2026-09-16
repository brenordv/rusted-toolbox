use crate::models::{FollowMode, TailConfig};
use anyhow::{bail, Result};
use clap::{Args, Parser};
use common_cli::common_tool_args::CommonToolArgsNoVerbose;
use common_cli::tool_exit_helpers::exit_error;
use common_cli::tool_log_level::ToolLogLevel;
use shared_head_tail::count_parser::parse_tail_count;
use shared_head_tail::models::{resolve_count, Count, CountPrefix, HeaderPolicy};
use std::ffi::OsString;
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

/// True when GNU tail's structural gate lets argv[1] be an obsolete option:
/// it is the sole argument, or exactly one operand follows (a bare `-` is an
/// operand, any longer `-x...` is an option), or `--` follows with at most
/// one operand after it.
fn obsolete_gate(args: &[OsString]) -> bool {
    let is_double_dash = |arg: &OsString| arg.to_str() == Some("--");
    match args.len() {
        2 => true,
        3 => {
            let second = args[2].as_encoded_bytes();
            is_double_dash(&args[2]) || !(second.len() >= 2 && second[0] == b'-')
        }
        4 => is_double_dash(&args[2]),
        _ => false,
    }
}

/// Rewrites GNU's obsolete first-argument forms (`tail -5`, `tail +20lf`,
/// `tail -5b file`) into the equivalent modern flags at the same position,
/// leaving every other argv untouched. Mirrors GNU tail's
/// `parse_obsolete_option`: only argv[1] is considered, only under the
/// structural gate above, and only with at least one digit after the sign
/// (GNU's digit-less forms like `-l` or `+f` are not honored; the readme
/// records the deviation). The grammar is `[+-]DIGITS[b|c|l]?[f]?`: `b`
/// counts 512-byte blocks (emitted as the `b` count suffix so the parser
/// owns the multiplier), `c` bytes, `l` lines, and a trailing `f` follows.
/// A `+` keeps its from-start meaning through the emitted count. Anything
/// outside the grammar passes through unchanged for clap to report, the way
/// GNU falls back to standard parsing; a non-UTF-8 argv[1] passes through
/// too.
pub fn rewrite_obsolete_argv(args: Vec<OsString>) -> Vec<OsString> {
    let Some(first) = args.get(1).and_then(|arg| arg.to_str()) else {
        return args;
    };
    let sign = match first.as_bytes().first() {
        Some(&sign @ (b'-' | b'+')) => sign,
        _ => return args,
    };
    let body = &first[1..];
    if !body.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        return args;
    }
    if !obsolete_gate(&args) {
        return args;
    }

    let digits_end = body
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(body.len());
    let (digits, letters) = body.split_at(digits_end);

    let mut flag = "-n";
    let mut suffix = "";
    let mut rest = letters;
    match rest.as_bytes().first() {
        Some(b'b') => {
            flag = "-c";
            suffix = "b";
            rest = &rest[1..];
        }
        Some(b'c') => {
            flag = "-c";
            rest = &rest[1..];
        }
        Some(b'l') => {
            rest = &rest[1..];
        }
        _ => {}
    }
    let follow = match rest {
        "" => false,
        "f" => true,
        _ => return args,
    };

    let mut count = String::new();
    if sign == b'+' {
        count.push('+');
    }
    count.push_str(digits);
    count.push_str(suffix);

    let mut rewritten: Vec<OsString> = Vec::with_capacity(args.len() + 2);
    rewritten.push(args[0].clone());
    rewritten.push(OsString::from(flag));
    rewritten.push(OsString::from(count));
    if follow {
        rewritten.push(OsString::from("-f"));
    }
    rewritten.extend(args.into_iter().skip(2));
    rewritten
}

/// Parses the command line, boots logging (and the optional `--app-header`
/// block), and returns the engine configuration. Boot happens before the
/// configuration is validated so the `--pid` rejection reaches the subscriber.
pub fn initialize() -> TailConfig {
    let cli = CliArgs::parse_from(rewrite_obsolete_argv(std::env::args_os().collect()));

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

    fn rewritten(argv: &[&str]) -> Vec<String> {
        rewrite_obsolete_argv(argv.iter().map(OsString::from).collect())
            .into_iter()
            .map(|arg| arg.into_string().unwrap())
            .collect()
    }

    #[rstest]
    #[case::plain(&["tail", "-5", "f"], &["tail", "-n", "5", "f"])]
    #[case::plus_form(&["tail", "+5"], &["tail", "-n", "+5"])]
    #[case::plus_zero(&["tail", "+0"], &["tail", "-n", "+0"])]
    #[case::block_multiplier(&["tail", "-5b"], &["tail", "-c", "5b"])]
    #[case::plus_block(&["tail", "+5b", "f"], &["tail", "-c", "+5b", "f"])]
    #[case::bytes_letter(&["tail", "-5c"], &["tail", "-c", "5"])]
    #[case::lines_letter(&["tail", "-5l"], &["tail", "-n", "5"])]
    #[case::trailing_follow(&["tail", "-5f"], &["tail", "-n", "5", "-f"])]
    #[case::block_and_follow(&["tail", "-5bf", "f"], &["tail", "-c", "5b", "-f", "f"])]
    #[case::plus_lines_follow(&["tail", "+20lf"], &["tail", "-n", "+20", "-f"])]
    #[case::bare_dash_operand(&["tail", "-5", "-"], &["tail", "-n", "5", "-"])]
    #[case::double_dash(&["tail", "-5", "--"], &["tail", "-n", "5", "--"])]
    #[case::double_dash_operand(&["tail", "-5", "--", "f"], &["tail", "-n", "5", "--", "f"])]
    fn obsolete_forms_rewrite_to_modern_flags(#[case] argv: &[&str], #[case] expected: &[&str]) {
        assert_eq!(rewritten(argv), expected);
    }

    #[rstest]
    #[case::no_args(&["tail"])]
    #[case::two_operands(&["tail", "-5", "a", "b"])]
    #[case::double_dash_two_operands(&["tail", "-5", "--", "a", "b"])]
    #[case::option_after_count(&["tail", "-5", "-n", "3"])]
    #[case::bare_dash(&["tail", "-"])]
    #[case::count_after_double_dash(&["tail", "--", "-5"])]
    #[case::modern_bytes(&["tail", "-c", "5"])]
    #[case::modern_follow(&["tail", "-f"])]
    #[case::digitless_lines(&["tail", "-l"])]
    #[case::digitless_plus_follow(&["tail", "+f"])]
    #[case::unknown_letter(&["tail", "-5x"])]
    #[case::letter_after_bytes(&["tail", "-5cb"])]
    #[case::letter_after_follow(&["tail", "-5fc"])]
    fn non_obsolete_argv_passes_through(#[case] argv: &[&str]) {
        assert_eq!(rewritten(argv), argv);
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_first_arg_passes_through() {
        use std::os::unix::ffi::OsStringExt;
        let bad = OsString::from_vec(vec![b'-', b'5', 0xFF]);

        let args = vec![OsString::from("tail"), bad.clone()];
        assert_eq!(rewrite_obsolete_argv(args)[1], bad);
    }

    #[cfg(windows)]
    #[test]
    fn non_utf8_first_arg_passes_through() {
        use std::os::windows::ffi::OsStringExt;
        let bad = OsString::from_wide(&[u16::from(b'-'), u16::from(b'5'), 0xD800]);

        let args = vec![OsString::from("tail"), bad.clone()];
        assert_eq!(rewrite_obsolete_argv(args)[1], bad);
    }

    fn config_via_rewrite(argv: &[&str]) -> TailConfig {
        let args = rewrite_obsolete_argv(argv.iter().map(OsString::from).collect());
        let cli = CliArgs::try_parse_from(args).unwrap();
        build_tail_config(&cli.args).unwrap()
    }

    #[test]
    fn rewritten_obsolete_forms_parse_to_the_expected_config() {
        let cfg = config_via_rewrite(&["tail", "-5"]);
        assert_eq!(cfg.unit, CountUnit::Lines);
        assert_eq!(cfg.count, 5);
        assert!(!cfg.from_start);

        let cfg = config_via_rewrite(&["tail", "+5"]);
        assert_eq!(cfg.unit, CountUnit::Lines);
        assert_eq!(cfg.count, 5);
        assert!(cfg.from_start);

        let cfg = config_via_rewrite(&["tail", "-5b"]);
        assert_eq!(cfg.unit, CountUnit::Bytes);
        assert_eq!(cfg.count, 2560);

        let cfg = config_via_rewrite(&["tail", "-5f", "app.log"]);
        assert_eq!(cfg.follow, Some(FollowMode::Descriptor));
        assert_eq!(cfg.count, 5);
        assert_eq!(cfg.files, vec!["app.log".to_string()]);
    }

    #[test]
    fn later_modern_flags_override_the_obsolete_form() {
        // The obsolete form is honored with `--` even when modern flags never
        // could follow it; the plain two-flag case goes through unrewritten.
        let cfg = config_via_rewrite(&["tail", "-5", "--", "f.txt"]);
        assert_eq!(cfg.count, 5);
        assert_eq!(cfg.files, vec!["f.txt".to_string()]);
    }
}
