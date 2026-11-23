//! The first run: with no settings file, speak asks where the speech server is, or refuses
//! when nobody can answer.

use crate::config::{DEFAULT_PORT, SpeakConfig};
use anyhow::{Context, Result, bail};
use std::io::{BufRead, Write};

/// What a run with no settings file says when it cannot ask.
pub const FIRST_RUN_REFUSAL: &str = "speak has no speech server set yet. Run `speak` once in a terminal to answer its two \
     questions, or run `speak --gui` and save the server in Settings.";

/// What a run with no settings file does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Ask the two questions on the terminal.
    Ask,
    /// Fail with [`FIRST_RUN_REFUSAL`].
    Refuse,
}

/// Asks only when a person can both read the questions (they are written to stderr) and type
/// the answers (they are read from stdin).
pub fn first_run_route(stdin_terminal: bool, stderr_terminal: bool) -> Route {
    if stdin_terminal && stderr_terminal {
        Route::Ask
    } else {
        Route::Refuse
    }
}

/// Asks for the server address and its port on `writer`, reads one line each from `reader`,
/// and returns the default settings pointed at that server. An empty port line means
/// [`DEFAULT_PORT`].
///
/// # Errors
/// When a question cannot be written or an answer read, the address is refused by
/// [`SpeakConfig::for_server`], or the port is not a number from 1 to 65535.
pub fn ask(mut reader: impl BufRead, mut writer: impl Write) -> Result<SpeakConfig> {
    let address = question(&mut reader, &mut writer, "Speech server address: ")?;
    let port = question(
        &mut reader,
        &mut writer,
        &format!("Port [{DEFAULT_PORT}]: "),
    )?;
    let port = parse_port(&port)?;
    SpeakConfig::for_server(&address, port)
}

fn question(reader: &mut impl BufRead, writer: &mut impl Write, prompt: &str) -> Result<String> {
    writer
        .write_all(prompt.as_bytes())
        .and_then(|()| writer.flush())
        .context("Failed to write the question")?;
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .context("Failed to read the answer")?;
    Ok(line.trim().to_owned())
}

fn parse_port(answer: &str) -> Result<u16> {
    if answer.is_empty() {
        return Ok(DEFAULT_PORT);
    }
    match answer.parse::<u16>() {
        Ok(port) if port != 0 => Ok(port),
        _ => bail!("The port must be a number from 1 to 65535"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::io::{self, Cursor, Read};

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("the terminal went away"))
        }
    }

    impl BufRead for FailingReader {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            Err(io::Error::other("the terminal went away"))
        }

        fn consume(&mut self, _: usize) {}
    }

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("stderr is closed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn answers(text: &str) -> Cursor<Vec<u8>> {
        Cursor::new(text.as_bytes().to_vec())
    }

    #[test]
    fn an_empty_port_line_means_the_default_port() {
        let mut shown = Vec::new();

        let config = ask(answers("my-host\n\n"), &mut shown).unwrap();

        assert_eq!(config.url, "http://my-host:10157/v1/audio/speech");
        assert_eq!(
            String::from_utf8(shown).unwrap(),
            "Speech server address: Port [10157]: "
        );
    }

    #[test]
    fn an_explicit_port_is_used() {
        let config = ask(answers("my-host\n8080\n"), Vec::new()).unwrap();

        assert_eq!(config.url, "http://my-host:8080/v1/audio/speech");
    }

    #[rstest]
    #[case::not_a_number("abc")]
    #[case::too_large("65536")]
    #[case::zero("0")]
    #[case::negative("-1")]
    fn a_port_that_is_not_a_port_number_is_refused(#[case] port: &str) {
        let error = ask(answers(&format!("my-host\n{port}\n")), Vec::new()).unwrap_err();

        assert!(error.to_string().contains("1 to 65535"), "{error:#}");
    }

    #[rstest]
    #[case::empty_line("\n\n")]
    #[case::only_spaces("   \n\n")]
    #[case::end_of_input("")]
    fn a_blank_address_is_refused(#[case] input: &str) {
        let error = ask(answers(input), Vec::new()).unwrap_err();

        assert!(error.to_string().contains("blank"), "{error:#}");
    }

    #[test]
    fn the_line_break_and_surrounding_spaces_are_stripped() {
        let config = ask(answers("  my-host \r\n 8080 \r\n"), Vec::new()).unwrap();

        assert_eq!(config.url, "http://my-host:8080/v1/audio/speech");
    }

    #[test]
    fn an_address_holding_a_control_character_is_refused() {
        let error = ask(answers("my\u{1b}[2Jhost\n\n"), Vec::new()).unwrap_err();

        assert!(
            error.to_string().contains("control characters"),
            "{error:#}"
        );
    }

    #[test]
    fn an_answer_that_cannot_be_read_is_an_error() {
        let error = ask(FailingReader, Vec::new()).unwrap_err();

        assert!(format!("{error:#}").contains("Failed to read the answer"));
    }

    #[test]
    fn a_question_that_cannot_be_written_is_an_error() {
        let error = ask(answers("my-host\n\n"), FailingWriter).unwrap_err();

        assert!(format!("{error:#}").contains("Failed to write the question"));
    }

    #[rstest]
    #[case::both_terminals(true, true, Route::Ask)]
    #[case::piped_stdin(false, true, Route::Refuse)]
    #[case::redirected_stderr(true, false, Route::Refuse)]
    #[case::neither(false, false, Route::Refuse)]
    fn first_run_route_asks_only_with_both_terminals(
        #[case] stdin_terminal: bool,
        #[case] stderr_terminal: bool,
        #[case] route: Route,
    ) {
        assert_eq!(first_run_route(stdin_terminal, stderr_terminal), route);
    }
}
