//! Logging boot for a tool that opens a window: nobody watches a window's stderr, so the log
//! file is on whatever the flags say, and a panic is logged before the default hook prints it.

use crate::app_logger::FileLogging;
use crate::common_tool_args::CommonToolArgsNoVerbose;
use crate::tool_log_level::ToolLogLevel;
use std::any::Any;
use std::panic::{self, Location};
use std::thread;
use tracing::error;

/// The thread name a panic report gives a thread that has none.
pub const UNNAMED_THREAD: &str = "<unnamed>";
/// The location a panic report gives a panic that names none.
pub const UNKNOWN_LOCATION: &str = "<unknown>";
/// The message a panic report gives a panic whose payload is not text.
pub const NO_MESSAGE: &str = "<a panic with no text message>";

/// `args` with the file sink on, whatever `--log-to-file` said; every other flag as given.
pub fn with_file_logging(args: CommonToolArgsNoVerbose) -> CommonToolArgsNoVerbose {
    CommonToolArgsNoVerbose {
        log_to_file: true,
        ..args
    }
}

/// What the panic hook [`boot_gui`] installs logs about a panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanicReport {
    /// The panicking thread's name.
    pub thread: String,
    /// `file:line:column` of the panic.
    pub location: String,
    /// The panic's text.
    pub message: String,
}

impl PanicReport {
    /// The report for a panic on the thread named `thread` at `location` with `payload`. A
    /// payload is text when it is a `&str` or a `String`, which is what `panic!` builds.
    pub fn new(
        thread: Option<&str>,
        location: Option<&Location<'_>>,
        payload: &(dyn Any + Send),
    ) -> Self {
        let message = payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| NO_MESSAGE.to_owned());
        Self {
            thread: thread.unwrap_or(UNNAMED_THREAD).to_owned(),
            location: location.map_or_else(
                || UNKNOWN_LOCATION.to_owned(),
                |at| format!("{}:{}:{}", at.file(), at.line(), at.column()),
            ),
            message,
        }
    }
}

/// Boots logging for a windowed tool: [`with_file_logging`] applied to `args`, at `level`
/// unless `--log-level` says otherwise, then a panic hook that writes one `error!` with the
/// [`PanicReport`] fields and hands the panic on to the hook that was installed before it.
/// Returns what the file sink is doing, so the tool can show the log path.
pub fn boot_gui(
    app_name: &str,
    app_version: &str,
    level: ToolLogLevel,
    args: CommonToolArgsNoVerbose,
) -> FileLogging {
    let file_logging = with_file_logging(args).app_boot_up_with_level(
        app_name,
        app_version,
        level,
        "<unused>",
        false,
        None::<fn()>,
    );
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let report = PanicReport::new(thread::current().name(), info.location(), info.payload());
        error!(
            thread = %report.thread,
            location = %report.location,
            message = ?report.message,
            "a thread panicked"
        );
        previous(info);
    }));
    file_logging
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_logging_is_forced_on_and_the_other_flags_are_kept() {
        let args = CommonToolArgsNoVerbose {
            log_level: Some(ToolLogLevel::Debug),
            log_to_stdout: true,
            rotate_log_file_by_day: true,
            ..CommonToolArgsNoVerbose::default()
        };

        let forced = with_file_logging(args);

        assert!(forced.log_to_file);
        assert_eq!(forced.log_level, Some(ToolLogLevel::Debug));
        assert!(forced.log_to_stdout);
        assert!(forced.rotate_log_file_by_day);
    }

    #[test]
    fn file_logging_already_on_stays_on() {
        let args = CommonToolArgsNoVerbose {
            log_to_file: true,
            ..CommonToolArgsNoVerbose::default()
        };

        assert!(with_file_logging(args).log_to_file);
    }

    #[test]
    fn a_report_carries_the_thread_the_location_and_a_str_message() {
        let at = Location::caller();
        let payload: &(dyn Any + Send) = &"worker lost its channel";

        let report = PanicReport::new(Some("seal-worker"), Some(at), payload);

        assert_eq!(
            report,
            PanicReport {
                thread: "seal-worker".to_owned(),
                location: format!("{}:{}:{}", at.file(), at.line(), at.column()),
                message: "worker lost its channel".to_owned(),
            }
        );
    }

    #[test]
    fn a_formatted_panic_message_is_read_from_its_string_payload() {
        let payload: &(dyn Any + Send) = &format!("index {} out of range", 7);

        let report = PanicReport::new(Some("main"), None, payload);

        assert_eq!(report.message, "index 7 out of range");
    }

    #[test]
    fn a_missing_name_location_and_text_get_their_placeholders() {
        let payload: &(dyn Any + Send) = &42_u32;

        let report = PanicReport::new(None, None, payload);

        assert_eq!(
            (
                report.thread.as_str(),
                report.location.as_str(),
                report.message.as_str()
            ),
            (UNNAMED_THREAD, UNKNOWN_LOCATION, NO_MESSAGE)
        );
    }
}
