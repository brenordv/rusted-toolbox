//! Test support: services that record each job and a state with a recipient book.

use crate::config::{RecipientEntry, SealConfig};
use crate::services::Services;
use crate::state::AppState;
use crate::worker::{JobRequest, WORKER_GONE};
use common_cli::app_logger::FileLogging;
use shared_crypto::keys::{generate_identity, public_key_of};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Services that keep every job they are asked to run, or refuse each one as a gone worker.
#[derive(Default)]
pub struct Recording {
    runs: RefCell<Vec<JobRequest>>,
    /// When set, `run` answers as a gone worker does.
    pub refuse: Cell<bool>,
}

impl Recording {
    /// How many jobs were run.
    pub fn run_count(&self) -> usize {
        self.runs.borrow().len()
    }

    /// The jobs run so far, oldest first, taken out.
    pub fn take_runs(&self) -> Vec<JobRequest> {
        self.runs.take()
    }
}

impl Services for Recording {
    fn run(&self, request: JobRequest) -> anyhow::Result<()> {
        if self.refuse.get() {
            anyhow::bail!(WORKER_GONE);
        }
        self.runs.borrow_mut().push(request);
        Ok(())
    }
}

/// `state` with fresh recording services.
pub fn recording_state(mut state: AppState) -> (AppState, Rc<Recording>) {
    let services = Rc::new(Recording::default());
    state.services = Some(services.clone());
    (state, services)
}

/// A window whose book holds `labels`, each with a fresh key.
pub fn state_with_book(labels: &[&str]) -> AppState {
    let recipients = labels
        .iter()
        .map(|label| RecipientEntry {
            label: (*label).to_owned(),
            public_key: public_key_of(&generate_identity()),
        })
        .collect();
    AppState::new(
        SealConfig {
            recipients,
            ..SealConfig::default()
        },
        FileLogging::Off,
        None,
        None,
    )
}
