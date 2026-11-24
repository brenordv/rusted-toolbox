use clap::Parser;
use common_cli::app_logger::FileLogging;
use common_cli::common_tool_args::CommonToolArgsNoVerbose;
use common_cli::gui_boot::boot_gui;
use common_cli::tool_log_level::ToolLogLevel;

/// Desktop GUI for seal: key-based file encryption over the age format.
///
/// Opens a window; the flags below only tune logging. File logging is on by default because a
/// windowed app has no visible stderr.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
pub struct CliArgs {
    #[command(flatten)]
    pub common: CommonToolArgsNoVerbose,
}

/// Parses the CLI flags and boots logging before any window exists, with the window boot: the
/// file sink forced on, info by default, and panics logged. `--log-level disabled` still
/// silences everything. Returns what the file sink is doing, for the Keys tab.
pub fn initialize() -> FileLogging {
    let args = CliArgs::parse();
    boot_gui(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        ToolLogLevel::Info,
        args.common,
    )
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
    fn bare_invocation_parses_with_the_logging_defaults() {
        let args = CliArgs::try_parse_from(["seal-gui"]).unwrap();

        assert_eq!(args.common.log_level, None);
        assert!(!args.common.log_to_file);
        assert!(!args.common.log_to_stdout);
        assert!(!args.common.app_header);
    }

    #[test]
    fn log_level_flag_parses_case_insensitively() {
        let args = CliArgs::try_parse_from(["seal-gui", "--log-level", "DEBUG"]).unwrap();

        assert_eq!(args.common.log_level, Some(ToolLogLevel::Debug));
    }

    #[test]
    fn a_rune_flag_is_a_usage_error() {
        let error = CliArgs::try_parse_from(["seal-gui", "--tree", "x"]).unwrap_err();

        assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
    }
}
