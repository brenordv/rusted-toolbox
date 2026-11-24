//! The work the window hands out, and `dispatch`, which applies an input and runs what it
//! asks for.

use crate::state::{AppState, DialogPurpose, Effect, Input};
use crate::views::focus_key;
use crate::worker::{JobEvent, JobRequest, WORKER_GONE, WorkerSubmit};
use rune::{FileDialogOptions, FileDialogResult, Tx};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};
use tracing::{debug, warn};

/// The work the window hands out.
pub trait Services {
    /// Queues a job on the worker; it answers later through the worker's posts.
    ///
    /// # Errors
    /// When the worker is gone and the job cannot start.
    fn run(&self, request: JobRequest) -> anyhow::Result<()>;
}

/// The services of a real window: the worker.
pub struct ProdServices {
    worker: WorkerSubmit,
}

impl ProdServices {
    pub fn new(worker: WorkerSubmit) -> Self {
        Self { worker }
    }
}

impl Services for ProdServices {
    fn run(&self, request: JobRequest) -> anyhow::Result<()> {
        self.worker.run(request)
    }
}

/// Lets the worker's posts through until the window closes. Closing and forwarding take the
/// same lock, so once [`PostGate::close`] returns, nothing more reaches the window.
#[derive(Clone, Default)]
pub struct PostGate(Arc<Mutex<bool>>);

impl PostGate {
    /// Shuts the gate; later posts are dropped.
    pub fn close(&self) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = true;
    }

    /// Runs `deliver` while the gate is open, holding the gate's lock across it.
    pub fn forward(&self, deliver: impl FnOnce()) {
        let closed = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if !*closed {
            deliver();
        }
    }
}

/// Applies `input`, runs every effect it returns, applies each answer an effect gives at
/// once, and runs that answer's effects, until none are left. Then it moves focus where the
/// last transition that named a place asked. True when the view must re-run.
pub fn dispatch(s: &mut AppState, tx: &mut Tx, input: Input) -> bool {
    let mut pending = VecDeque::from([input]);
    let mut changed = false;
    let mut focus = None;
    while let Some(input) = pending.pop_front() {
        let outcome = s.apply(input);
        changed |= outcome.changed;
        if outcome.focus.is_some() {
            focus = outcome.focus;
        }
        for effect in outcome.effects {
            if let Some(answer) = run_effect(s, tx, effect) {
                pending.push_back(answer);
            }
        }
    }
    if let Some(focus) = focus {
        let key = focus_key(focus);
        tx.focus(&key, true);
        tx.scroll_into_view(&key);
    }
    changed
}

/// Runs one effect; the input it answers with at once, if any.
fn run_effect(s: &AppState, tx: &mut Tx, effect: Effect) -> Option<Input> {
    match effect {
        Effect::Run(request) => {
            let started = match &s.services {
                Some(services) => services.run(request),
                None => Err(anyhow::anyhow!(WORKER_GONE)),
            };
            match started {
                Ok(()) => None,
                Err(error) => Some(Input::Job(JobEvent::Failed(format!("{error:#}")))),
            }
        }
        Effect::OpenDialog(purpose) => {
            open_dialog(tx, purpose);
            None
        }
        Effect::Copy(text) => {
            tx.copy(text);
            None
        }
    }
}

/// Shows the file dialog for `purpose`; its answer comes back as an input.
fn open_dialog(tx: &mut Tx, purpose: DialogPurpose) {
    debug!(purpose = purpose.as_str(), "a file dialog opened");
    let answer = move |s: &mut AppState, result: FileDialogResult, tx: &mut Tx| {
        if let Err(error) = &result {
            warn!(purpose = purpose.as_str(), error = %error, "a file dialog failed");
        }
        dispatch(s, tx, Input::DialogAnswered(purpose, result));
    };
    match purpose {
        DialogPurpose::Identity(_) => tx.open_file(
            FileDialogOptions {
                title: "Choose identity file".to_owned(),
                ..FileDialogOptions::default()
            },
            answer,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn an_open_gate_forwards_and_a_closed_one_drops() {
        let gate = PostGate::default();
        let forwarded = Cell::new(0);

        gate.forward(|| forwarded.set(forwarded.get() + 1));
        gate.close();
        gate.forward(|| forwarded.set(forwarded.get() + 1));

        assert_eq!(forwarded.get(), 1);
    }

    #[test]
    fn a_clone_of_the_gate_closes_with_it() {
        let gate = PostGate::default();
        let posted_through = gate.clone();
        let forwarded = Cell::new(false);

        gate.close();
        posted_through.forward(|| forwarded.set(true));

        assert!(!forwarded.get());
    }
}
