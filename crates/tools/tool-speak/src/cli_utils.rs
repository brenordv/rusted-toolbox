use crate::models::SpeakArgs;
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgsNoVerbose;
use common_cli::tool_log_level::ToolLogLevel;

/// Speaks text aloud through a text-to-speech server.
///
/// Sends the text to the server set on the first run and plays the audio it answers with.
/// The text comes from TEXT, or from stdin when TEXT is absent and stdin is piped. speak
/// returns when the audio ends. The server address is asked on the first run in a terminal
/// and kept in ~/.rusted-toolbox/tool-speak/config.json; no flag sets it.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
pub struct CliArgs {
    /// The text to speak; read from stdin when absent and stdin is piped
    pub text: Option<String>,

    /// Model for this run, in place of the configured one (nothing is saved)
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,

    /// Voice for this run, in place of the configured one (nothing is saved)
    #[arg(long, value_name = "VOICE")]
    pub voice: Option<String>,

    #[command(flatten)]
    pub common: CommonToolArgsNoVerbose,
}

/// Parses the command line and boots logging at the house default: warnings and errors on
/// stderr.
pub fn initialize() -> SpeakArgs {
    let args = CliArgs::parse();
    args.common.app_boot_up_with_level(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        ToolLogLevel::Warn,
        "<unused>",
        false,
        None::<fn()>,
    );
    SpeakArgs {
        text: args.text,
        model: args.model,
        voice: args.voice,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use rstest::rstest;

    #[test]
    fn cli_definition_has_no_conflicting_flags() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn a_bare_invocation_parses_with_no_text_and_no_overrides() {
        let args = CliArgs::try_parse_from(["speak"]).unwrap();

        assert_eq!((args.text, args.model, args.voice), (None, None, None));
    }

    #[test]
    fn text_and_the_two_overrides_parse() {
        let args =
            CliArgs::try_parse_from(["speak", "--model", "m", "--voice", "v.wav", "Hello there"])
                .unwrap();

        assert_eq!(args.text.as_deref(), Some("Hello there"));
        assert_eq!(args.model.as_deref(), Some("m"));
        assert_eq!(args.voice.as_deref(), Some("v.wav"));
    }

    #[rstest]
    #[case::url("--url")]
    #[case::config("--config")]
    #[case::save_config("--save-config")]
    #[case::host("--host")]
    #[case::port("--port")]
    fn no_flag_sets_saves_or_selects_the_server_address(#[case] flag: &str) {
        let result = CliArgs::try_parse_from(["speak", flag, "value", "text"]);

        assert_eq!(
            result.unwrap_err().kind(),
            clap::error::ErrorKind::UnknownArgument
        );
    }
}
