use crate::models::HeadConfig;
use clap::error::ErrorKind;
use clap::{Args, CommandFactory, Parser};
use common_cli::common_tool_args::CommonToolArgsNoVerbose;
use common_cli::tool_log_level::ToolLogLevel;
use shared_head_tail::count_parser::parse_head_count;
use shared_head_tail::models::{resolve_count, Count, CountPrefix, HeaderPolicy};
use std::ffi::OsString;

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

/// Rewrites GNU's obsolete first-argument form (`head -5`, `head -2qv`,
/// `head -5k`) into the equivalent modern flags at the same position, leaving
/// every other argv untouched. Only the first argument is inspected, and only
/// when it starts with `-` followed by an ASCII digit, the same condition GNU
/// head applies to `argv[1]`. The trailing letters mutate state left to right
/// as in GNU: `b`/`k`/`m` switch to bytes and set a multiplier suffix, `c`
/// switches to bytes and clears the multiplier, `l` switches to lines while
/// keeping any multiplier, and `q`/`v`/`z` splice the matching flags in order
/// so clap's last-wins overrides resolve them like GNU's getopt pass.
/// A first argument that is not valid UTF-8 is passed through unchanged.
///
/// # Errors
/// Fails on a trailing character outside GNU's set (`b`, `k`, `m`, `l`, `c`,
/// `q`, `v`, `z`), naming the offending character.
pub fn rewrite_obsolete_argv(args: Vec<OsString>) -> Result<Vec<OsString>, String> {
    let Some(first) = args.get(1).and_then(|arg| arg.to_str()) else {
        return Ok(args);
    };
    if !first.starts_with('-') || !first.as_bytes().get(1).is_some_and(u8::is_ascii_digit) {
        return Ok(args);
    }

    let body = &first[1..];
    let digits_end = body
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(body.len());
    let (digits, letters) = body.split_at(digits_end);

    let mut count_lines = true;
    let mut multiplier: Option<char> = None;
    let mut flags: Vec<&str> = Vec::new();
    for letter in letters.chars() {
        match letter {
            'b' | 'k' | 'm' => {
                count_lines = false;
                multiplier = Some(letter);
            }
            'c' => {
                count_lines = false;
                multiplier = None;
            }
            'l' => count_lines = true,
            'q' => flags.push("-q"),
            'v' => flags.push("-v"),
            'z' => flags.push("-z"),
            other => return Err(format!("invalid trailing option -- '{other}'")),
        }
    }

    let mut count = String::from(digits);
    count.extend(multiplier);

    let mut rewritten: Vec<OsString> = Vec::with_capacity(args.len() + flags.len() + 2);
    rewritten.push(args[0].clone());
    rewritten.push(OsString::from(if count_lines { "-n" } else { "-c" }));
    rewritten.push(OsString::from(count));
    rewritten.extend(flags.into_iter().map(OsString::from));
    rewritten.extend(args.into_iter().skip(2));
    Ok(rewritten)
}

/// Parses the command line, boots logging (and the optional `--app-header`
/// block), and returns the engine configuration.
pub fn initialize() -> HeadConfig {
    let args = match rewrite_obsolete_argv(std::env::args_os().collect()) {
        Ok(args) => args,
        Err(message) => CliArgs::command()
            .error(ErrorKind::ValueValidation, message)
            .exit(),
    };
    let cli = CliArgs::parse_from(args);

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
    use rstest::rstest;
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

    fn rewritten(argv: &[&str]) -> Vec<String> {
        rewrite_obsolete_argv(argv.iter().map(OsString::from).collect())
            .unwrap()
            .into_iter()
            .map(|arg| arg.into_string().unwrap())
            .collect()
    }

    #[rstest]
    #[case::plain(&["head", "-5", "f"], &["head", "-n", "5", "f"])]
    #[case::block_multiplier(&["head", "-5b"], &["head", "-c", "5b"])]
    #[case::kibi_multiplier(&["head", "-5k"], &["head", "-c", "5k"])]
    #[case::mebi_multiplier(&["head", "-5m"], &["head", "-c", "5m"])]
    #[case::bytes_plain(&["head", "-5c"], &["head", "-c", "5"])]
    #[case::later_c_clears_multiplier(&["head", "-5kc"], &["head", "-c", "5"])]
    #[case::multiplier_survives_l(&["head", "-5kl"], &["head", "-n", "5k"])]
    #[case::later_l_wins(&["head", "-3cl"], &["head", "-n", "3"])]
    #[case::later_c_wins(&["head", "-3lc"], &["head", "-c", "3"])]
    #[case::flag_letters_in_order(&["head", "-2qvz", "f"], &["head", "-n", "2", "-q", "-v", "-z", "f"])]
    #[case::verbose_then_quiet(&["head", "-2vq"], &["head", "-n", "2", "-v", "-q"])]
    fn obsolete_forms_rewrite_to_modern_flags(#[case] argv: &[&str], #[case] expected: &[&str]) {
        assert_eq!(rewritten(argv), expected);
    }

    #[rstest]
    #[case::no_args(&["head"])]
    #[case::double_dash(&["head", "--", "-5"])]
    #[case::bare_dash(&["head", "-"])]
    #[case::modern_flag(&["head", "-n", "5"])]
    #[case::second_position(&["head", "f", "-5"])]
    #[case::plus_form(&["head", "+5"])]
    fn non_obsolete_argv_passes_through(#[case] argv: &[&str]) {
        assert_eq!(rewritten(argv), argv);
    }

    #[rstest]
    #[case::uppercase("-5X", 'X')]
    #[case::unknown_lowercase("-5x", 'x')]
    #[case::multi_byte("-5é", 'é')]
    fn invalid_trailing_letter_is_named(#[case] arg: &str, #[case] letter: char) {
        let error =
            rewrite_obsolete_argv(vec![OsString::from("head"), OsString::from(arg)]).unwrap_err();
        assert_eq!(error, format!("invalid trailing option -- '{letter}'"));
    }

    #[cfg(windows)]
    #[test]
    fn non_utf8_first_arg_passes_through() {
        use std::os::windows::ffi::OsStringExt;
        let bad = OsString::from_wide(&[u16::from(b'-'), u16::from(b'5'), 0xD800]);

        let args = vec![OsString::from("head"), bad.clone()];
        assert_eq!(rewrite_obsolete_argv(args).unwrap()[1], bad);
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_first_arg_passes_through() {
        use std::os::unix::ffi::OsStringExt;
        let bad = OsString::from_vec(vec![b'-', b'5', 0xFF]);

        let args = vec![OsString::from("head"), bad.clone()];
        assert_eq!(rewrite_obsolete_argv(args).unwrap()[1], bad);
    }

    fn config_via_rewrite(argv: &[&str]) -> HeadConfig {
        let args = rewrite_obsolete_argv(argv.iter().map(OsString::from).collect()).unwrap();
        let cli = CliArgs::try_parse_from(args).unwrap();
        build_head_config(&cli.args)
    }

    #[test]
    fn rewritten_obsolete_forms_parse_to_the_expected_config() {
        let cfg = config_via_rewrite(&["head", "-5"]);
        assert_eq!(cfg.unit, CountUnit::Lines);
        assert_eq!(cfg.count, 5);

        let cfg = config_via_rewrite(&["head", "-5k"]);
        assert_eq!(cfg.unit, CountUnit::Bytes);
        assert_eq!(cfg.count, 5120);

        let cfg = config_via_rewrite(&["head", "-5kl"]);
        assert_eq!(cfg.unit, CountUnit::Lines);
        assert_eq!(cfg.count, 5120);

        let cfg = config_via_rewrite(&["head", "-2qv"]);
        assert_eq!(cfg.headers, HeaderPolicy::Always);
        assert!(!cfg.zero_terminated);
    }

    #[test]
    fn later_modern_flags_override_the_obsolete_form() {
        let cfg = config_via_rewrite(&["head", "-5", "-n", "3"]);
        assert_eq!(cfg.unit, CountUnit::Lines);
        assert_eq!(cfg.count, 3);
    }
}
