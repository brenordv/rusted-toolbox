//! The Keys tab's transitions, and the one function that says which inputs drop its confirms.

use super::keys::{AddIssue, KeygenStep, KeysInput, clean_label, new_entry};
use super::notes::{Copied, Missing, Note, RunningJob};
use super::recipients::EntryId;
use super::{AppState, DialogPurpose, Effect, Focus, Input, Outcome, Tab};
use crate::worker::JobRequest;
use rune::TextEdit;
use std::path::PathBuf;

/// Whether `input`, arriving while `showing` is the tab on screen, drops the Keys tab's two
/// confirms (the replace confirm and the armed delete) before it applies. The user's inputs on
/// the Keys tab do, and so does a switch away from it. Generate, Delete, and Replace handle the
/// confirms in their own transitions: Generate changes nothing while the replace confirm
/// shows, Delete arms or confirms its row, and Replace is the replace confirm. A switch to Keys,
/// background answers, the other tabs' inputs, and a drag over the Files list leave them armed.
pub fn disarms(input: &Input, showing: Tab) -> bool {
    match input {
        Input::TabPicked(to) => showing == Tab::Keys && *to != Tab::Keys,
        Input::Keys(
            KeysInput::GeneratePressed | KeysInput::DeletePressed(_) | KeysInput::ReplacePressed,
        ) => false,
        Input::Keys(
            KeysInput::CancelPressed
            | KeysInput::CopyGenerated
            | KeysInput::AddGeneratedPressed
            | KeysInput::CopyEntry(_)
            | KeysInput::LabelEdited
            | KeysInput::KeyEdited
            | KeysInput::AddPressed,
        ) => true,
        Input::Text(_)
        | Input::Files(_)
        | Input::Inspected(_)
        | Input::OutputsProbed(_)
        | Input::DialogOpened(_)
        | Input::DialogAnswered(..)
        | Input::Job(_)
        | Input::Stopped
        | Input::ConfigSaved(_)
        | Input::Closing => false,
    }
}

impl AppState {
    pub(super) fn keys_input(&mut self, input: KeysInput, out: &mut Outcome) {
        match input {
            KeysInput::GeneratePressed => self.generate(out),
            KeysInput::ReplacePressed => self.replace(out),
            KeysInput::CancelPressed => {
                out.focus = Some(Focus::Generate);
                out.changed = true;
            }
            KeysInput::CopyGenerated => {
                if let Some(generated) = &self.keys.generated {
                    let key = generated.public_key.clone();
                    self.copy_key(key, out);
                }
            }
            KeysInput::CopyEntry(id) => {
                if let Some(entry) = self.book.get(id) {
                    let key = entry.public_key.clone();
                    self.copy_key(key, out);
                }
            }
            KeysInput::DeletePressed(id) => self.delete_pressed(id, out),
            KeysInput::LabelEdited | KeysInput::KeyEdited => {
                if self.keys.issue.take().is_some() {
                    out.changed = true;
                }
            }
            KeysInput::AddPressed => self.add_from_form(out),
            KeysInput::AddGeneratedPressed => self.add_generated(out),
        }
    }

    /// The save dialog for a new identity file opens, unless the replace confirm shows, a job
    /// runs, or another dialog is open. Pressing it drops an armed delete.
    fn generate(&mut self, out: &mut Outcome) {
        match self.keys.step {
            KeygenStep::Idle => {}
            KeygenStep::ConfirmReplace { .. } => return,
        }
        if self.blocked().is_some() || self.dialog.is_some() {
            return;
        }
        if self.keys.armed.take().is_some() {
            out.changed = true;
        }
        self.dialog_opened(DialogPurpose::SaveIdentity, out);
    }

    /// The save dialog answered a path: a keygen that will not replace a file starts, unless
    /// the worker is busy or stopped, when the path is dropped (the status line already names
    /// the running job, or the standing caption the stop).
    pub(super) fn keygen_path_chosen(&mut self, path: PathBuf, out: &mut Outcome) {
        if self.blocked().is_some() {
            return;
        }
        self.accept_start(
            RunningJob::Keygen,
            JobRequest::Keygen {
                path,
                replace: false,
            },
            out,
        );
    }

    /// The replace confirm's Replace: the keygen that replaces the file starts, when nothing
    /// runs; while a job runs the confirm stays.
    fn replace(&mut self, out: &mut Outcome) {
        let KeygenStep::ConfirmReplace { path } = &self.keys.step else {
            return;
        };
        if self.blocked().is_some() {
            return;
        }
        let path = path.clone();
        self.keys.step = KeygenStep::Idle;
        self.keys.armed = None;
        self.accept_start(
            RunningJob::Keygen,
            JobRequest::Keygen {
                path,
                replace: true,
            },
            out,
        );
        out.focus = Some(Focus::Generate);
    }

    fn copy_key(&mut self, key: String, out: &mut Outcome) {
        out.effects.push(Effect::Copy(key));
        self.notes.set(Tab::Keys, Note::Copied(Copied::PublicKey));
        out.changed = true;
    }

    /// Delete on a row arms it, dropping any other confirm; on the armed row it removes the
    /// entry from the book and from both pickers, and the notes and the add form's issue that
    /// named it go.
    fn delete_pressed(&mut self, id: EntryId, out: &mut Outcome) {
        let Some(removed_label) = self.book.get(id).map(|entry| clean_label(&entry.label)) else {
            return;
        };
        self.keys.step = KeygenStep::Idle;
        out.changed = true;
        if self.keys.armed != Some(id) {
            self.keys.armed = Some(id);
            return;
        }
        self.keys.armed = None;
        let Some(index) = self.book.remove(id) else {
            return;
        };
        self.text.picker.selected.remove(&id);
        self.files.picker.selected.remove(&id);
        self.notes.clear_where(|_, note| {
            matches!(note, Note::Missing(Missing::SavedKey { id: named, .. }) if *named == id)
        });
        if self.keys.issue == Some(AddIssue::DuplicateLabel(removed_label)) {
            self.keys.issue = None;
        }
        self.save_config(out);
        // Focus stays in the book: the row that took this one's place, else the one before it,
        // else the add form.
        let entries = self.book.entries();
        let next = entries
            .get(index)
            .or_else(|| index.checked_sub(1).and_then(|before| entries.get(before)));
        out.focus = Some(next.map_or(Focus::AddLabel, |entry| Focus::Delete(entry.id)));
    }

    fn add_from_form(&mut self, out: &mut Outcome) {
        out.changed = true;
        let label = self.keys.label.text.to_string();
        let key = self.keys.key.text.to_string();
        match new_entry(&self.book, &label, &key) {
            Ok((label, key)) => {
                self.keys.label = TextEdit::new("");
                self.keys.key = TextEdit::new("");
                self.keys.issue = None;
                self.add_entry(label, key, out);
                out.focus = Some(Focus::AddLabel);
            }
            Err(issue) => {
                out.focus = Some(if issue.on_label() {
                    Focus::AddLabel
                } else {
                    Focus::AddKey
                });
                self.keys.issue = Some(issue);
            }
        }
    }

    /// "Add to recipient book" on the generated key: the entry joins under the saved file's
    /// stem, as the form would add it; when the stem is empty or taken, the form takes the stem
    /// and the key and shows why.
    fn add_generated(&mut self, out: &mut Outcome) {
        let Some(generated) = self.keys.generated.clone() else {
            return;
        };
        if self
            .book
            .entries()
            .iter()
            .any(|entry| entry.public_key == generated.public_key)
        {
            return;
        }
        out.changed = true;
        let stem = generated.stem();
        match new_entry(&self.book, &stem, &generated.public_key) {
            Ok((label, key)) => {
                self.add_entry(label, key, out);
                out.focus = Some(Focus::Generate);
            }
            Err(issue) => {
                self.keys.label = TextEdit::new(stem.as_str());
                self.keys.key = TextEdit::new(generated.public_key.as_str());
                out.focus = Some(if issue.on_label() {
                    Focus::AddLabel
                } else {
                    Focus::AddKey
                });
                self.keys.issue = Some(issue);
            }
        }
    }

    /// Adds an entry the form's rules accepted: the note names it, the config is saved, and its
    /// row scrolls into view.
    fn add_entry(&mut self, label: String, key: String, out: &mut Outcome) {
        let id = self.book.push(label.clone(), key);
        self.notes.set(Tab::Keys, Note::Added(label));
        self.save_config(out);
        out.reveal = Some(Focus::Delete(id));
    }
}
