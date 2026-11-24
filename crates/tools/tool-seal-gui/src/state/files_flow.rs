//! The Files tab's transitions.

use super::files::{FilesInput, PathKind, files_job};
use super::notes::{Note, RunningJob};
use super::recipients::Picker;
use super::{AppState, Effect, Focus, Outcome, Tab};
use std::path::PathBuf;

impl AppState {
    pub(super) fn files_input(&mut self, input: FilesInput, out: &mut Outcome) {
        let running = self.running_on(Tab::Files);
        match input {
            FilesInput::ModePicked(mode) => {
                if running || self.files.mode == mode {
                    return;
                }
                self.files.mode = mode;
                self.files.revalidate();
                self.probe_outputs(out);
                out.changed = true;
            }
            FilesInput::OverwriteToggled => {
                self.files.overwrite = !self.files.overwrite;
                out.changed = true;
            }
            FilesInput::OutputPicked(choice) => {
                if self.files.out_choice == choice {
                    return;
                }
                self.files.out_choice = choice;
                self.probe_outputs(out);
                out.changed = true;
            }
            FilesInput::AddPaths(paths) => {
                if !paths.is_empty() {
                    out.effects.push(Effect::Inspect(paths));
                }
            }
            FilesInput::RemoveRow(path) => {
                if running {
                    return;
                }
                let Some(index) = self.files.rows.iter().position(|row| row.path == path) else {
                    return;
                };
                self.files.rows.remove(index);
                // Focus stays in the list: the row that took this one's place, else the one
                // before it, else the button that adds files.
                let count = self.files.rows.len();
                out.focus = Some(if count == 0 {
                    Focus::AddFiles
                } else {
                    Focus::RemoveRow(index.min(count - 1))
                });
                out.changed = true;
            }
            FilesInput::ClearList => {
                if running || self.files.rows.is_empty() {
                    return;
                }
                self.files.rows.clear();
                out.focus = Some(Focus::AddFiles);
                out.changed = true;
            }
            FilesInput::RecipientToggled(id) => {
                if self.book.get(id).is_some() {
                    self.files.picker.toggle(id);
                    out.changed = true;
                }
            }
            FilesInput::OtherKeyEdited => {
                self.files.picker.check_other_key();
                out.changed = true;
            }
            FilesInput::StartPressed => self.start_files(out),
            FilesInput::DropHover(on) => {
                if self.files.drop_hover != on {
                    self.files.drop_hover = on;
                    out.changed = true;
                }
            }
        }
    }

    /// What the intake's paths are: regular and unreadable ones become rows, and the note says
    /// why the first other one was turned away.
    pub(super) fn inspected(&mut self, inspected: Vec<(PathBuf, PathKind)>, out: &mut Outcome) {
        let before = self.files.rows.len();
        if let Some(skipped) = self.files.add_inspected(inspected) {
            self.notes.set(Tab::Files, Note::Skipped(skipped));
            out.changed = true;
        }
        if self.files.rows.len() > before {
            self.probe_outputs(out);
            out.changed = true;
        }
    }

    fn start_files(&mut self, out: &mut Outcome) {
        if self.blocked().is_some() {
            return;
        }
        match files_job(&self.files, &self.book, self.identity_path.as_ref()) {
            Ok(request) => {
                let job = RunningJob::Files {
                    direction: self.files.mode,
                    finished: 0,
                    total: self.files.rows.len(),
                };
                self.files.begin_run();
                self.accept_start(job, request, out);
            }
            Err(missing) => self.refuse_start(Picker::Files, missing, out),
        }
    }
}
