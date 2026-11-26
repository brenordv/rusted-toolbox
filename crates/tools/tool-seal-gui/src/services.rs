//! The work the window hands out, and `dispatch`, which applies an input and runs what it
//! asks for.

use crate::config::{ConfigStore, SaveOutcome, SealConfig};
use crate::state::files::PathKind;
use crate::state::{AppState, DialogPurpose, Effect, Input};
use crate::views::focus_key;
use crate::worker::{JobEvent, JobRequest, WORKER_GONE, WorkerSubmit};
use rune::{FileDialogOptions, FileDialogResult, Tx};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use tracing::{debug, error, warn};

/// The work the window hands out.
pub trait Services {
    /// Queues a job on the worker; it answers later through the worker's posts.
    ///
    /// # Errors
    /// When the worker is gone and the job cannot start.
    fn run(&self, request: JobRequest) -> anyhow::Result<()>;

    /// What each path is.
    fn inspect(&self, paths: Vec<PathBuf>) -> Vec<(PathBuf, PathKind)>;

    /// For each (row path, output path) pair, the row path and whether the output exists.
    fn probe(&self, pairs: Vec<(PathBuf, PathBuf)>) -> Vec<(PathBuf, bool)>;

    /// Writes `snapshot` to the config file.
    fn save_config(&self, snapshot: &SealConfig) -> SaveOutcome;
}

/// The services of a real window: the worker, the config file, and the file system for the
/// short checks the Files tab makes on the UI thread.
pub struct ProdServices {
    worker: WorkerSubmit,
    store: ConfigStore,
}

impl ProdServices {
    pub fn new(worker: WorkerSubmit, store: ConfigStore) -> Self {
        Self { worker, store }
    }
}

impl Services for ProdServices {
    fn run(&self, request: JobRequest) -> anyhow::Result<()> {
        self.worker.run(request)
    }

    fn save_config(&self, snapshot: &SealConfig) -> SaveOutcome {
        self.store.flush(snapshot)
    }

    fn inspect(&self, paths: Vec<PathBuf>) -> Vec<(PathBuf, PathKind)> {
        inspect_paths(paths)
    }

    fn probe(&self, pairs: Vec<(PathBuf, PathBuf)>) -> Vec<(PathBuf, bool)> {
        probe_outputs(pairs)
    }
}

/// Each path with what it is on disk.
pub fn inspect_paths(paths: Vec<PathBuf>) -> Vec<(PathBuf, PathKind)> {
    paths
        .into_iter()
        .map(|path| {
            let kind = path_kind(&path);
            (path, kind)
        })
        .collect()
}

/// Each (row path, output path) pair as the row path and whether the output exists.
pub fn probe_outputs(pairs: Vec<(PathBuf, PathBuf)>) -> Vec<(PathBuf, bool)> {
    pairs
        .into_iter()
        .map(|(row, output)| {
            let exists = output.exists();
            (row, exists)
        })
        .collect()
}

/// What `path` is, following links. A path whose metadata cannot be read is `Unreadable`; the
/// worker reports the real error when it runs.
pub fn path_kind(path: &Path) -> PathKind {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => PathKind::RegularFile { size: meta.len() },
        Ok(meta) if meta.is_dir() => PathKind::Folder,
        Ok(_) => PathKind::Other,
        Err(_) => PathKind::Unreadable,
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
    let mut reveal = None;
    while let Some(input) = pending.pop_front() {
        let outcome = s.apply(input);
        changed |= outcome.changed;
        if outcome.focus.is_some() {
            focus = outcome.focus;
        }
        if outcome.reveal.is_some() {
            reveal = outcome.reveal;
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
    if let Some(reveal) = reveal {
        tx.scroll_into_view(focus_key(reveal));
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
        Effect::Inspect(paths) => s
            .services
            .as_ref()
            .map(|services| Input::Inspected(services.inspect(paths))),
        Effect::ProbeOutputs(pairs) => s
            .services
            .as_ref()
            .map(|services| Input::OutputsProbed(services.probe(pairs))),
        Effect::SaveConfig(snapshot) => {
            let outcome = s.services.as_ref()?.save_config(&snapshot);
            match &outcome {
                SaveOutcome::Refused | SaveOutcome::NoLocation => {
                    error!(
                        outcome = ?outcome,
                        "a config save was asked of a store that never writes"
                    );
                }
                SaveOutcome::Saved | SaveOutcome::Failed(_) => {}
            }
            Some(Input::ConfigSaved(outcome))
        }
    }
}

/// The dialog for `purpose` answered: a failure is logged, then the answer is dispatched.
/// True when the view must re-run.
pub fn dialog_answered(
    s: &mut AppState,
    tx: &mut Tx,
    purpose: DialogPurpose,
    result: FileDialogResult,
) -> bool {
    if let Err(error) = &result {
        warn!(purpose = purpose.as_str(), error = %error, "a file dialog failed");
    }
    dispatch(s, tx, Input::DialogAnswered(purpose, result))
}

/// Shows the file dialog for `purpose`; its answer comes back as an input.
fn open_dialog(tx: &mut Tx, purpose: DialogPurpose) {
    debug!(purpose = purpose.as_str(), "a file dialog opened");
    let answer = move |s: &mut AppState, result: FileDialogResult, tx: &mut Tx| {
        dialog_answered(s, tx, purpose, result);
    };
    let titled = |title: &str| FileDialogOptions {
        title: title.to_owned(),
        ..FileDialogOptions::default()
    };
    match purpose {
        DialogPurpose::Identity(_) => tx.open_file(titled("Choose identity file"), answer),
        DialogPurpose::AddFile => tx.open_file(titled("Add files"), answer),
        DialogPurpose::OutputFolder => tx.open_folder(titled("Choose output folder"), answer),
        DialogPurpose::SaveIdentity => tx.save_file(
            FileDialogOptions {
                file_name: Some("seal-identity.txt".to_owned()),
                ..titled("Save identity file")
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
    fn i1_inspect_names_a_file_with_its_size_a_folder_and_a_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, b"hello").unwrap();
        let missing = dir.path().join("gone.txt");

        let kinds = inspect_paths(vec![
            file.clone(),
            dir.path().to_path_buf(),
            missing.clone(),
        ]);

        assert_eq!(
            kinds,
            [
                (file, PathKind::RegularFile { size: 5 }),
                (dir.path().to_path_buf(), PathKind::Folder),
                (missing, PathKind::Unreadable),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn i1_inspect_follows_a_link_and_names_a_socket_other() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        std::fs::write(&file, b"hello").unwrap();
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        let socket = dir.path().join("socket");
        let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();

        assert_eq!(path_kind(&link), PathKind::RegularFile { size: 5 });
        assert_eq!(path_kind(&socket), PathKind::Other);
    }

    #[test]
    fn i2_probe_answers_each_pair_by_its_row() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("a.txt.age");
        std::fs::write(&existing, b"sealed").unwrap();

        let probed = probe_outputs(vec![
            (PathBuf::from("a.txt"), existing),
            (PathBuf::from("b.txt"), dir.path().join("b.txt.age")),
        ]);

        assert_eq!(
            probed,
            [
                (PathBuf::from("a.txt"), true),
                (PathBuf::from("b.txt"), false)
            ]
        );
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
