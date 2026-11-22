//! Three prompts (a choice from a list, a checked line of text, and a yes or
//! no), each answered or ended by Esc, Ctrl-C, or a terminal that can't be
//! read, and the terminal that asks them through inquire.

use anyhow::anyhow;
use inquire::error::InquireResult;
use inquire::validator::Validation;
use inquire::{Confirm, CustomUserError, InquireError, Select, Text};

/// What one prompt gave back.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Answer<T> {
    Value(T),
    /// Esc.
    Cancelled,
    /// Ctrl-C.
    Interrupted,
    /// The terminal can't be read: raw mode could not be enabled.
    Unavailable,
}

/// A check on a typed answer; `Err` holds the message shown under the prompt, which stays
/// open until the answer passes.
pub(crate) type Check = fn(&str) -> Result<(), String>;

/// A choice from a list, a checked line of text, and a yes or no. Every text passed in is
/// already escaped for the terminal.
pub(crate) trait Prompter {
    /// One of `options`, by index, with the cursor starting on `start`.
    fn select(
        &mut self,
        prompt: &str,
        options: &[String],
        start: usize,
    ) -> anyhow::Result<Answer<usize>>;

    /// A line of text, starting as `initial`, that must pass `check`.
    fn text(&mut self, prompt: &str, initial: &str, check: Check)
    -> anyhow::Result<Answer<String>>;

    /// Yes or no, with `default` on Enter.
    fn confirm(&mut self, prompt: &str, default: bool) -> anyhow::Result<Answer<bool>>;
}

/// The help line under a list: the keys that move, pick, and cancel.
const SELECT_HELP: &str = "↑↓ to move, enter to select, esc to cancel";

/// The terminal, through inquire. inquire draws on stderr.
pub(crate) struct TerminalPrompter;

impl Prompter for TerminalPrompter {
    fn select(
        &mut self,
        prompt: &str,
        options: &[String],
        start: usize,
    ) -> anyhow::Result<Answer<usize>> {
        let chosen = Select::new(prompt, options.to_vec())
            .with_starting_cursor(start)
            .without_filtering()
            .with_help_message(SELECT_HELP)
            .raw_prompt();
        answer(chosen.map(|option| option.index))
    }

    fn text(
        &mut self,
        prompt: &str,
        initial: &str,
        check: Check,
    ) -> anyhow::Result<Answer<String>> {
        let typed = Text::new(prompt)
            .with_initial_value(initial)
            .with_validator(move |input: &str| validation(check(input)))
            .prompt();
        answer(typed)
    }

    fn confirm(&mut self, prompt: &str, default: bool) -> anyhow::Result<Answer<bool>> {
        answer(Confirm::new(prompt).with_default(default).prompt())
    }
}

/// A check's result as inquire's validation.
fn validation(checked: Result<(), String>) -> Result<Validation, CustomUserError> {
    Ok(match checked {
        Ok(()) => Validation::Valid,
        Err(message) => Validation::Invalid(message.into()),
    })
}

/// A prompt's result as an [`Answer`]: Esc, Ctrl-C, and a terminal that can't be read are
/// answers, and inquire's other errors stay errors.
///
/// # Errors
/// Fails on an I/O error, an invalid prompt configuration, or a check that failed to run.
pub(crate) fn answer<T>(result: InquireResult<T>) -> anyhow::Result<Answer<T>> {
    match result {
        Ok(value) => Ok(Answer::Value(value)),
        Err(InquireError::OperationCanceled) => Ok(Answer::Cancelled),
        Err(InquireError::OperationInterrupted) => Ok(Answer::Interrupted),
        Err(InquireError::NotTTY) => Ok(Answer::Unavailable),
        Err(InquireError::IO(error)) => {
            Err(anyhow::Error::new(error).context("The menu could not use the terminal"))
        }
        Err(InquireError::InvalidConfiguration(message)) => Err(anyhow!(
            "The menu built a prompt inquire refused: {message}"
        )),
        Err(InquireError::Custom(error)) => Err(anyhow!("A menu check failed to run: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::io;

    #[rstest]
    #[case::canceled(InquireError::OperationCanceled, Answer::Cancelled)]
    #[case::interrupted(InquireError::OperationInterrupted, Answer::Interrupted)]
    #[case::not_a_terminal(InquireError::NotTTY, Answer::Unavailable)]
    fn the_handled_errors_become_answers(
        #[case] error: InquireError,
        #[case] expected: Answer<u8>,
    ) {
        assert_eq!(answer::<u8>(Err(error)).unwrap(), expected);
    }

    #[test]
    fn a_value_is_the_answer() {
        assert_eq!(answer(Ok(7_u8)).unwrap(), Answer::Value(7));
    }

    #[rstest]
    #[case::io(
        InquireError::IO(io::Error::other("closed")),
        "The menu could not use the terminal: closed"
    )]
    #[case::configuration(
        InquireError::InvalidConfiguration("no options".to_owned()),
        "The menu built a prompt inquire refused: no options"
    )]
    #[case::custom(
        InquireError::Custom("check panicked".into()),
        "A menu check failed to run: check panicked"
    )]
    fn the_other_errors_end_the_menu(#[case] error: InquireError, #[case] expected: &str) {
        let failed = answer::<u8>(Err(error)).unwrap_err();

        assert_eq!(format!("{failed:#}"), expected);
    }

    #[test]
    fn a_passing_check_is_valid() {
        assert_eq!(validation(Ok(())).unwrap(), Validation::Valid);
    }

    #[test]
    fn a_failing_check_shows_its_message() {
        assert_eq!(
            validation(Err("Type a number".to_owned())).unwrap(),
            Validation::Invalid("Type a number".into())
        );
    }
}
