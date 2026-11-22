//! A prompter that answers from a script and records what it was asked. It
//! fails the test on a prompt with no reply left, on a reply that does not fit
//! the prompt, and, when dropped, on a reply nobody asked for.

use crate::interactive::prompter::{Answer, Check, Prompter};
use std::collections::VecDeque;

/// One scripted reply.
#[derive(Debug)]
pub(crate) enum Reply {
    Pick(usize),
    /// Typed into a text prompt; a reply its check refuses is recorded as
    /// [`Asked::Rejected`] and the prompt takes the next reply.
    Type(&'static str),
    Yes,
    No,
    Esc,
    CtrlC,
    NoTerminal,
    /// The prompt fails with an error.
    Broken,
}

/// One prompt the menu showed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Asked {
    Select {
        prompt: String,
        options: Vec<String>,
        start: usize,
    },
    Text {
        prompt: String,
        initial: String,
    },
    Rejected {
        prompt: String,
        message: String,
    },
    Confirm {
        prompt: String,
        default: bool,
    },
}

pub(crate) struct ScriptedPrompter {
    replies: VecDeque<Reply>,
    pub(crate) asked: Vec<Asked>,
}

impl ScriptedPrompter {
    pub(crate) fn new(replies: impl IntoIterator<Item = Reply>) -> Self {
        Self {
            replies: replies.into_iter().collect(),
            asked: Vec::new(),
        }
    }

    fn next(&mut self, prompt: &str) -> Reply {
        self.replies
            .pop_front()
            .unwrap_or_else(|| panic!("no reply left for {prompt:?}"))
    }
}

fn stop<T>(reply: Reply, prompt: &str) -> anyhow::Result<Answer<T>> {
    match reply {
        Reply::Esc => Ok(Answer::Cancelled),
        Reply::CtrlC => Ok(Answer::Interrupted),
        Reply::NoTerminal => Ok(Answer::Unavailable),
        Reply::Broken => Err(anyhow::anyhow!("the terminal went away")),
        reply @ (Reply::Pick(_) | Reply::Type(_) | Reply::Yes | Reply::No) => {
            panic!("{reply:?} does not answer {prompt:?}")
        }
    }
}

impl Prompter for ScriptedPrompter {
    fn select(
        &mut self,
        prompt: &str,
        options: &[String],
        start: usize,
    ) -> anyhow::Result<Answer<usize>> {
        self.asked.push(Asked::Select {
            prompt: prompt.to_owned(),
            options: options.to_vec(),
            start,
        });
        match self.next(prompt) {
            Reply::Pick(index) => Ok(Answer::Value(index)),
            reply => stop(reply, prompt),
        }
    }

    fn text(
        &mut self,
        prompt: &str,
        initial: &str,
        check: Check,
    ) -> anyhow::Result<Answer<String>> {
        self.asked.push(Asked::Text {
            prompt: prompt.to_owned(),
            initial: initial.to_owned(),
        });
        loop {
            match self.next(prompt) {
                Reply::Type(typed) => match check(typed) {
                    Ok(()) => return Ok(Answer::Value(typed.to_owned())),
                    Err(message) => self.asked.push(Asked::Rejected {
                        prompt: prompt.to_owned(),
                        message,
                    }),
                },
                reply => return stop(reply, prompt),
            }
        }
    }

    fn confirm(&mut self, prompt: &str, default: bool) -> anyhow::Result<Answer<bool>> {
        self.asked.push(Asked::Confirm {
            prompt: prompt.to_owned(),
            default,
        });
        match self.next(prompt) {
            Reply::Yes => Ok(Answer::Value(true)),
            Reply::No => Ok(Answer::Value(false)),
            reply => stop(reply, prompt),
        }
    }
}

impl Drop for ScriptedPrompter {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(
                self.replies.is_empty(),
                "replies nobody asked for: {:?}",
                self.replies
            );
        }
    }
}
