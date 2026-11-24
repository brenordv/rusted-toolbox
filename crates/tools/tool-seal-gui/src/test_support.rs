//! Test support: services that record each job and a state with a recipient book.

use crate::config::{RecipientEntry, SealConfig};
use crate::services::Services;
use crate::state::AppState;
use crate::state::files::PathKind;
use crate::worker::{JobRequest, WORKER_GONE};
use common_cli::app_logger::FileLogging;
use shared_crypto::keys::{generate_identity, public_key_of};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

/// Services that keep every job they are asked to run, or refuse each one as a gone worker, and
/// answer the file-system checks from what a test set.
#[derive(Default)]
pub struct Recording {
    runs: RefCell<Vec<JobRequest>>,
    /// When set, `run` answers as a gone worker does.
    pub refuse: Cell<bool>,
    /// What `inspect` says each path is; any other path is a one-byte regular file.
    pub kinds: RefCell<HashMap<PathBuf, PathKind>>,
    /// The outputs `probe` says exist.
    pub existing: RefCell<HashSet<PathBuf>>,
    /// Each file-system check in order: its name and how many paths it was asked about.
    pub checks: RefCell<Vec<(&'static str, usize)>>,
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

    /// The file-system checks so far, oldest first, taken out.
    pub fn take_checks(&self) -> Vec<(&'static str, usize)> {
        self.checks.take()
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

    fn inspect(&self, paths: Vec<PathBuf>) -> Vec<(PathBuf, PathKind)> {
        self.checks.borrow_mut().push(("inspect", paths.len()));
        let kinds = self.kinds.borrow();
        paths
            .into_iter()
            .map(|path| {
                let kind = kinds
                    .get(&path)
                    .copied()
                    .unwrap_or(PathKind::RegularFile { size: 1 });
                (path, kind)
            })
            .collect()
    }

    fn probe(&self, pairs: Vec<(PathBuf, PathBuf)>) -> Vec<(PathBuf, bool)> {
        self.checks.borrow_mut().push(("probe", pairs.len()));
        let existing = self.existing.borrow();
        pairs
            .into_iter()
            .map(|(row, output)| {
                let exists = existing.contains(&output);
                (row, exists)
            })
            .collect()
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
