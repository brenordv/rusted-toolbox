//! What the dep-check window asks of the outside world: one check at a time, each answered by
//! exactly one [`Input::Checked`].

use crate::state::{DepCheckState, Effect, Input};
use dep_check_core::engine::{CheckRequest, Checker, system_clock};
use dep_check_core::report::Report;
use rune::{AppHandle, Tx};
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::task::JoinError;
use tracing::{error, warn};

/// What a check is answered with when the window has no services.
pub const NOT_SET_UP: &str = "dep-check-gui is not set up";

/// The work the window's effects ask for. Each call returns at once; its answer arrives later
/// as an [`Input`].
pub trait Services {
    /// Runs `request`; answers [`Input::Checked`] exactly once.
    fn check(&self, request: CheckRequest);
}

/// Applies `input` to `s`, moves focus where the transition says, and hands the effects to
/// the window's services; true when anything the view reads changed.
pub fn dispatch(s: &mut DepCheckState, tx: &mut Tx, input: Input) -> bool {
    let transition = s.apply(input);
    let mut changed = transition.changed;
    if let Some(key) = transition.focus {
        tx.focus(key, true);
        tx.scroll_into_view(key);
    }
    for effect in transition.effects {
        changed |= run_effect(s, tx, effect);
    }
    changed
}

/// Hands `effect` to the services. A window with none answers the check with a failure at
/// once, so it still gets its one answer; true when that changed the view.
fn run_effect(s: &mut DepCheckState, tx: &mut Tx, effect: Effect) -> bool {
    let Some(services) = s.services.clone() else {
        error!("the dep-check window's services are not set up");
        let answer = match effect {
            Effect::Check(_) => Input::Checked(Err(NOT_SET_UP.to_owned())),
        };
        return dispatch(s, tx, answer);
    };
    match effect {
        Effect::Check(request) => services.check(request),
    }
    false
}

/// One check on `checker`, on the system clock. The engine logs its start and its end.
pub async fn run_check(checker: Arc<Checker>, request: CheckRequest) -> Report {
    checker.check(request, system_clock()).await
}

/// The answer a check task gave, or, when the task died before giving one, the failure made of
/// that. Either way the check gets exactly one answer.
pub fn answer_of(joined: Result<Report, JoinError>) -> Input {
    match joined {
        Ok(report) => Input::Checked(Ok(Box::new(report))),
        Err(error) => {
            let text = format!("The check task stopped unexpectedly: {error}");
            warn!(outcome = "stopped", error = %text, "check finished");
            Input::Checked(Err(text))
        }
    }
}

/// The services of a running window: each check runs on the tokio runtime, and its answer is
/// applied on the UI thread through the app handle.
pub struct ProdServices {
    handle: AppHandle<DepCheckState>,
    rt: Handle,
    checker: Arc<Checker>,
}

impl ProdServices {
    /// Services that run checks on `checker` on `rt` and post their answers to `handle`.
    pub fn new(handle: AppHandle<DepCheckState>, rt: Handle, checker: Arc<Checker>) -> Self {
        Self {
            handle,
            rt,
            checker,
        }
    }
}

impl Services for ProdServices {
    fn check(&self, request: CheckRequest) {
        let handle = self.handle.clone();
        let task = run_check(Arc::clone(&self.checker), request);
        self.rt.spawn(async move {
            let answer = answer_of(tokio::spawn(task).await);
            handle.update_if(move |s: &mut DepCheckState, tx: &mut Tx| dispatch(s, tx, answer));
        });
    }
}

#[cfg(test)]
mod tests;
