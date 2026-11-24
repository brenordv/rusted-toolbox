use clap::Parser;
use common_cli::app_logger::FileLogging;
use common_cli::common_tool_args::CommonToolArgsNoVerbose;
use common_cli::gui_boot::boot_gui;
use common_cli::tool_log_level::ToolLogLevel;

/// Desktop GUI for Azure Event Hubs: live capture into a local archive, archive search, and
/// export.
///
/// Opens a window; the flags below only tune logging. File logging is on by default because a
/// windowed app has no visible stderr.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
struct CliArgs {
    #[command(flatten)]
    common: CommonToolArgsNoVerbose,
}

/// Parses the CLI flags and boots logging before any window exists, with the window boot: the
/// file sink forced on, info by default, and panics logged, inverting the house defaults. The log
/// file is the only place a windowed app's failures land. `--log-level disabled` still silences
/// everything. Returns what the file sink is doing, for the connection details.
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
        let args = CliArgs::try_parse_from(["eh-gui"]).unwrap();

        assert_eq!(args.common.log_level, None);
        assert!(!args.common.log_to_file);
    }
}
