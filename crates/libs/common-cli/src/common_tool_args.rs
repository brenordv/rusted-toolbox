use crate::app_logger::AppLogger;
use crate::broken_pipe::{BrokenPipe, write_out};
use crate::header_format::{format_config_section, render_standard_header};
use crate::tool_log_level::ToolLogLevel;
use clap::Args;
use std::io::Write;
use tracing::{debug, warn};

/// The CLI flags shared by every tool: logging level and channels, the header
/// toggle, and verbose mode. Flattened into each tool's argument parser with
/// `#[command(flatten)]`.
///
/// Keep the shared flag definitions in sync with [`CommonToolArgsNoVerbose`];
/// the sync is pinned by a test in this module.
#[derive(Args, Debug)]
#[command(about, long_about, version)]
pub struct CommonToolArgs {
    /// Shows the header with tool name, version, and runtime options
    #[arg(long = "app-header")]
    pub app_header: bool,

    /// Enables verbose output
    #[arg(long = "verbose")]
    pub verbose: bool,

    /// Sets the log level. Output goes to stderr by default with no channel flag
    /// needed; `disabled` silences everything and overrides `RUST_LOG`, while
    /// every other level yields to `RUST_LOG` when it is set.
    #[arg(long = "log-level", default_value_t = ToolLogLevel::Warn, ignore_case = true)]
    pub default_logging_level: ToolLogLevel,

    /// Log to stdout instead of the default stderr
    #[arg(long = "log-to-console")]
    pub log_to_stdout: bool,

    /// If the tool should log to a file
    #[arg(long = "log-to-file")]
    pub log_to_file: bool,

    /// Rotate the log file by day
    #[arg(long = "rotate-log-file-by-day")]
    pub rotate_log_file_by_day: bool,
}

impl CommonToolArgs {
    /// Builds the [` AppLogger `] for this tool from the parsed CLI flags without
    /// installing it. Callers that hand the logger's layers to an externally
    /// owned subscriber use this seam before initialization.
    pub fn build_logger(&self, app_name: &str) -> AppLogger {
        AppLogger::new(
            app_name,
            self.default_logging_level.clone(),
            self.log_to_stdout,
            self.log_to_file,
            self.rotate_log_file_by_day,
        )
    }

    fn initialize_logging(&self, app_name: &str, force_disable: bool) {
        self.build_logger(app_name).init(force_disable);
    }

    /// Boots the tool: initializes logging, then prints the standard header and
    /// runtime-config block when `--app-header` is set. `tool_header_printer`, when
    /// provided, appends a tool-specific config section under the shared block.
    ///
    /// The header bytes come from `header_format::render_standard_header`, whose
    /// tests pin the exact output.
    pub fn app_boot_up(
        &self,
        app_name: &str,
        app_version: &str,
        uses_verbose_flag: bool,
        force_disable_log: bool,
        tool_header_printer: Option<impl FnOnce()>,
    ) {
        self.initialize_logging(app_name, force_disable_log);

        if !self.app_header {
            return;
        }

        let header = if uses_verbose_flag {
            self.render_header(app_name, app_version, self.verbose)
        } else {
            self.render_header(app_name, app_version, "<unused>")
        };
        print_header_block(&header, tool_header_printer);
    }

    fn render_header(
        &self,
        app_name: &str,
        app_version: &str,
        verbose: impl std::fmt::Display,
    ) -> String {
        render_standard_header(
            app_name,
            app_version,
            verbose,
            &self.default_logging_level,
            self.log_to_stdout,
            self.log_to_file,
            self.rotate_log_file_by_day,
        )
    }
}

/// Prints the rendered standard header, the optional tool-specific config
/// section, and the trailing blank line. Both boot paths print through here so
/// the `--app-header` byte layout cannot drift between them.
///
/// The header is informational, so a failing stdout never fails or panics the
/// boot: a closed pipe skips the rest of the block (the tool's own printer
/// included, since it would hit the same dead pipe) with a debug note, and any
/// other write error skips it with one warning naming the error.
fn print_header_block(header: &str, tool_header_printer: Option<impl FnOnce()>) {
    if let Err(error) = write_header_block(&mut std::io::stdout(), header, tool_header_printer) {
        if error.is::<BrokenPipe>() {
            debug!("--app-header block skipped: stdout closed by the consumer");
        } else {
            warn!("--app-header block not printed: {error:#}");
        }
    }
}

/// Writes the shared header-block lines to `out`, calling the tool printer
/// between the section line and the trailing blank line. Split from
/// [`print_header_block`] so tests can inject a writer; the tool printer still
/// prints through its own means, which is the production arrangement too.
///
/// # Errors
/// Fails with the first write error, [`BrokenPipe`]-marked when the consumer
/// closed the pipe; the remaining lines and the tool printer are skipped.
fn write_header_block<W: Write>(
    out: &mut W,
    header: &str,
    tool_header_printer: Option<impl FnOnce()>,
) -> anyhow::Result<()> {
    write_out(out, format!("{header}\n").as_bytes())?;

    if let Some(printer) = tool_header_printer {
        write_out(
            out,
            format!("{}\n", format_config_section("Tool Runtime Config")).as_bytes(),
        )?;
        printer();
    }

    write_out(out, b"\n")
}

/// The shared CLI flags for a tool that owns its own verbosity flag and derives
/// its own default log level. Three differences from [`CommonToolArgs`]: there
/// is no `--verbose` (the tool defines its own, typically a `-v` count flag; a
/// second `--verbose` in the same command is a clap debug panic), `--log-level`
/// carries no baked default, so `None` means "not passed" and the tool supplies
/// its derived default through
/// [`app_boot_up_with_level`](Self::app_boot_up_with_level), and there are no
/// `#[command(about, long_about, version)]` attributes, so a flattening tool
/// never inherits those from this struct and must declare its own.
///
/// Keep the shared flag definitions in sync with [`CommonToolArgs`]; the sync
/// is pinned by a test in this module.
#[derive(Args, Clone, Debug, Default)]
pub struct CommonToolArgsNoVerbose {
    /// Shows the header with tool name, version, and runtime options
    #[arg(long = "app-header")]
    pub app_header: bool,

    /// Sets the log level; when omitted, the tool picks its own default.
    /// Output goes to stderr by default with no channel flag needed;
    /// `disabled` silences everything and overrides `RUST_LOG`, while every
    /// other level yields to `RUST_LOG` when it is set.
    #[arg(long = "log-level", ignore_case = true)]
    pub log_level: Option<ToolLogLevel>,

    /// Log to stdout instead of the default stderr
    #[arg(long = "log-to-console")]
    pub log_to_stdout: bool,

    /// If the tool should log to a file
    #[arg(long = "log-to-file")]
    pub log_to_file: bool,

    /// Rotate the log file by day
    #[arg(long = "rotate-log-file-by-day")]
    pub rotate_log_file_by_day: bool,
}

impl CommonToolArgsNoVerbose {
    /// Resolves the effective log level: an explicit `--log-level` wins over
    /// the tool's `derived` default, even when the tool derived that default
    /// from other flags (a silent mode, say).
    pub fn resolved_level(&self, derived: ToolLogLevel) -> ToolLogLevel {
        self.log_level.clone().unwrap_or(derived)
    }

    /// Boots the tool like [`CommonToolArgs::app_boot_up`], with the log level
    /// resolved by [`resolved_level`](Self::resolved_level) from the tool's
    /// `derived_level` and the optional explicit `--log-level`. The header's
    /// Verbose-mode line shows whatever `verbose` renders (a count, for a tool
    /// with a `-v` count flag), and the Log-level line shows the resolved
    /// effective level, never the raw flag.
    pub fn app_boot_up_with_level(
        &self,
        app_name: &str,
        app_version: &str,
        derived_level: ToolLogLevel,
        verbose: impl std::fmt::Display,
        force_disable_log: bool,
        tool_header_printer: Option<impl FnOnce()>,
    ) {
        let effective_level = self.resolved_level(derived_level);

        AppLogger::new(
            app_name,
            effective_level.clone(),
            self.log_to_stdout,
            self.log_to_file,
            self.rotate_log_file_by_day,
        )
        .init(force_disable_log);

        if !self.app_header {
            return;
        }

        let header = render_standard_header(
            app_name,
            app_version,
            verbose,
            &effective_level,
            self.log_to_stdout,
            self.log_to_file,
            self.rotate_log_file_by_day,
        );
        print_header_block(&header, tool_header_printer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_writers::{ClosedPipe, FailingDisk};
    use clap::Parser;

    #[test]
    fn write_header_block_pins_the_shared_bytes_with_a_tool_section() {
        let mut out: Vec<u8> = Vec::new();
        let mut printer_ran = false;

        write_header_block(&mut out, "HEADER", Some(|| printer_ran = true)).unwrap();

        assert!(printer_ran);
        let expected = format!(
            "HEADER\n{}\n\n",
            format_config_section("Tool Runtime Config")
        );
        assert_eq!(out, expected.as_bytes());
    }

    #[test]
    fn write_header_block_pins_the_shared_bytes_without_a_tool_section() {
        let mut out: Vec<u8> = Vec::new();

        write_header_block(&mut out, "HEADER", None::<fn()>).unwrap();

        assert_eq!(out, b"HEADER\n\n");
    }

    #[test]
    fn write_header_block_on_a_closed_pipe_skips_the_tool_printer() {
        let mut printer_ran = false;

        let error =
            write_header_block(&mut ClosedPipe, "HEADER", Some(|| printer_ran = true)).unwrap_err();

        assert!(error.is::<BrokenPipe>());
        assert!(!printer_ran, "the printer must not run on a dead pipe");
    }

    #[test]
    fn write_header_block_keeps_other_write_errors_unmarked() {
        let error = write_header_block(&mut FailingDisk, "HEADER", None::<fn()>).unwrap_err();

        assert!(!error.is::<BrokenPipe>());
    }

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(flatten)]
        common: CommonToolArgs,
    }

    #[test]
    fn defaults_are_warn_level_with_all_flags_off() {
        let cli = TestCli::parse_from(["test"]);

        // The default lands on the `Warning` synonym, not `Warn`: clap renders
        // `default_value_t` through Display ("Warning") and parses it back.
        // Both variants map to tracing's `warn`.
        assert_eq!(cli.common.default_logging_level, ToolLogLevel::Warning);
        assert_eq!(cli.common.default_logging_level.to_tracing_level(), "warn");
        assert!(!cli.common.app_header);
        assert!(!cli.common.verbose);
        assert!(!cli.common.log_to_stdout);
        assert!(!cli.common.log_to_file);
        assert!(!cli.common.rotate_log_file_by_day);
    }

    #[test]
    fn log_level_parses_case_insensitively() {
        let cli = TestCli::parse_from(["test", "--log-level", "DEBUG"]);

        assert_eq!(cli.common.default_logging_level, ToolLogLevel::Debug);
    }

    #[derive(Parser, Debug)]
    struct TestCliNoVerbose {
        #[command(flatten)]
        common: CommonToolArgsNoVerbose,
    }

    /// Mirrors the consumer shape the struct exists for: a tool-owned `-v`
    /// count flag next to the flatten.
    #[derive(Parser, Debug)]
    struct TestCliOwnedVerbose {
        #[arg(long = "verbose", short = 'v', action = clap::ArgAction::Count)]
        verbose: u8,

        #[command(flatten)]
        common: CommonToolArgsNoVerbose,
    }

    #[test]
    fn no_verbose_flatten_coexists_with_tool_owned_verbose_count() {
        use clap::CommandFactory;
        TestCliOwnedVerbose::command().debug_assert();

        let cli = TestCliOwnedVerbose::parse_from(["test", "-vv", "--log-level", "debug"]);

        assert_eq!(cli.verbose, 2);
        assert_eq!(cli.common.log_level, Some(ToolLogLevel::Debug));
    }

    #[test]
    fn no_verbose_defaults_match_default_impl() {
        let parsed = TestCliNoVerbose::parse_from(["test"]).common;
        let default = CommonToolArgsNoVerbose::default();

        // The list-style boot path substitutes Default::default() for a parse,
        // so the two must agree field for field.
        assert_eq!(parsed.log_level, default.log_level);
        assert_eq!(parsed.app_header, default.app_header);
        assert_eq!(parsed.log_to_stdout, default.log_to_stdout);
        assert_eq!(parsed.log_to_file, default.log_to_file);
        assert_eq!(
            parsed.rotate_log_file_by_day,
            default.rotate_log_file_by_day
        );
        assert_eq!(parsed.log_level, None);
        assert!(!parsed.app_header);
    }

    #[test]
    fn no_verbose_log_level_parses_case_insensitively() {
        let cli = TestCliNoVerbose::parse_from(["test", "--log-level", "DEBUG"]);

        assert_eq!(cli.common.log_level, Some(ToolLogLevel::Debug));
    }

    #[test]
    fn resolved_level_prefers_explicit_flag_over_derived() {
        let explicit = TestCliNoVerbose::parse_from(["test", "--log-level", "info"]).common;
        assert_eq!(
            explicit.resolved_level(ToolLogLevel::Error),
            ToolLogLevel::Info
        );

        let omitted = TestCliNoVerbose::parse_from(["test"]).common;
        assert_eq!(
            omitted.resolved_level(ToolLogLevel::Error),
            ToolLogLevel::Error
        );
    }

    #[test]
    fn shared_flag_definitions_stay_in_sync() {
        use clap::CommandFactory;
        let full = TestCli::command();
        let no_verbose = TestCliNoVerbose::command();

        for id in [
            "app_header",
            "log_to_stdout",
            "log_to_file",
            "rotate_log_file_by_day",
        ] {
            let in_full = full
                .get_arguments()
                .find(|arg| arg.get_id().as_str() == id)
                .expect("flag missing from CommonToolArgs");
            let in_no_verbose = no_verbose
                .get_arguments()
                .find(|arg| arg.get_id().as_str() == id)
                .expect("flag missing from CommonToolArgsNoVerbose");

            assert_eq!(in_full.get_long(), in_no_verbose.get_long());
            assert_eq!(
                in_full.get_help().map(ToString::to_string),
                in_no_verbose.get_help().map(ToString::to_string)
            );
        }

        assert!(
            no_verbose
                .get_arguments()
                .all(|arg| arg.get_id().as_str() != "verbose"),
            "CommonToolArgsNoVerbose must not define --verbose"
        );

        // The log-level field ids differ across the structs, so the loop above
        // cannot pair them; pin the shared long name explicitly.
        for command in [&full, &no_verbose] {
            assert!(
                command
                    .get_arguments()
                    .any(|arg| arg.get_long() == Some("log-level")),
                "both structs must expose --log-level"
            );
        }
    }
}
