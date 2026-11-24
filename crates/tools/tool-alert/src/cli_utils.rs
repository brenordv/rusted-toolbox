use crate::models::AlertArgs;
use clap::Parser;
use common_cli::common_tool_args::CommonToolArgsNoVerbose;
use common_cli::gui_boot::boot_gui;
use common_cli::tool_log_level::ToolLogLevel;

/// Shows a window with a message and waits for the user to close it.
///
/// The window stays above other windows and has one button, Close. alert returns when the
/// user closes it, so a caller that must not wait starts it in the background. A sound that
/// cannot play never hides the alert: the window says why and the exit code is still 0.
/// With --gui, alert opens a tester window instead, filled with any title, body, and sound
/// given.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about)]
pub struct CliArgs {
    /// The message; required unless --gui
    #[arg(required_unless_present = "gui")]
    pub body: Option<String>,

    /// A title shown above the message
    #[arg(short = 't', long, value_name = "TITLE")]
    pub title: Option<String>,

    /// A sound file to play when the alert shows (WAV, MP3, FLAC, Ogg Vorbis)
    #[arg(short = 's', long, value_name = "FILE")]
    pub sound: Option<String>,

    /// Open the tester window
    #[arg(short = 'g', long)]
    pub gui: bool,

    #[command(flatten)]
    pub common: CommonToolArgsNoVerbose,
}

impl CliArgs {
    /// The parsed values without the logging flags.
    pub fn into_alert_args(self) -> AlertArgs {
        AlertArgs {
            title: self.title,
            body: self.body,
            sound: self.sound,
            gui: self.gui,
        }
    }
}

/// Parses the command line and boots logging before any window exists, with the window boot:
/// the file sink forced on, info by default, and panics logged, inverting the house defaults.
/// An alert started in the background has no stderr anyone watches, so the log file is where
/// its shown and closed lines land. `--log-level disabled` still silences everything.
pub fn initialize() -> AlertArgs {
    let args = CliArgs::parse();
    boot_gui(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        ToolLogLevel::Info,
        args.common.clone(),
    );
    args.into_alert_args()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use clap::error::ErrorKind;

    #[test]
    fn cli_definition_has_no_conflicting_flags() {
        CliArgs::command().debug_assert();
    }

    #[test]
    fn a_body_alone_parses_with_no_title_no_sound_and_no_gui() {
        let args = CliArgs::try_parse_from(["alert", "Build finished"])
            .unwrap()
            .into_alert_args();

        assert_eq!(
            args,
            AlertArgs {
                title: None,
                body: Some("Build finished".to_owned()),
                sound: None,
                gui: false,
            }
        );
    }

    #[test]
    fn the_short_flags_set_the_title_the_sound_and_the_tester() {
        let args = CliArgs::try_parse_from(["alert", "-t", "Done", "-s", "ping.wav", "-g"])
            .unwrap()
            .into_alert_args();

        assert_eq!(
            args,
            AlertArgs {
                title: Some("Done".to_owned()),
                body: None,
                sound: Some("ping.wav".to_owned()),
                gui: true,
            }
        );
    }

    #[test]
    fn a_missing_body_without_gui_is_a_usage_error() {
        let result = CliArgs::try_parse_from(["alert", "--title", "Done"]);

        assert_eq!(
            result.unwrap_err().kind(),
            ErrorKind::MissingRequiredArgument
        );
    }
}
