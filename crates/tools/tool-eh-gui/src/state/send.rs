//! The app state's side of the Send section: the sheet's purposes, Send inputs, and where a
//! run's end is reported.

use super::{AppState, Effect, Outcome, Section, SheetPurpose};
use crate::send::{PressContext, SendInput, ended_toast};
use tracing::{debug, info};

impl AppState {
    /// Opens the connection sheet for `purpose`, unless what runs now must keep its target:
    /// a running send keeps every sheet closed, and a capture session keeps the target-only
    /// sheet closed (Send then goes to the hub being read).
    pub(super) fn open_sheet(&mut self, purpose: SheetPurpose, out: &mut Outcome) {
        if self.send.is_running() {
            debug!(?purpose, "sheet refused: a send is running");
            out.changed = false;
            return;
        }
        let session = self.connection.is_active();
        match purpose {
            SheetPurpose::SendTarget if session => {
                debug!("sheet refused: a capture session holds the target");
                out.changed = false;
            }
            SheetPurpose::SendTarget | SheetPurpose::Connect => self.sheet = Some(purpose),
        }
    }

    /// Closes the sheet (on any path), hides typed secrets, and recomputes the destination the
    /// form resolves to.
    pub(super) fn close_sheet(&mut self) {
        self.sheet = None;
        self.form.hide_secrets();
        self.send.refresh_destination(&self.form);
    }

    pub(super) fn send_input(&mut self, input: SendInput, out: &mut Outcome) {
        let cx = PressContext {
            sheet_open: self.sheet.is_some(),
            window_closed: self.window_closed,
            form: &self.form,
        };
        let send = self.send.apply(input, cx);
        out.effects
            .extend(send.effects.into_iter().map(Effect::Send));
        out.changed = send.changed;
        let Some((failed, line)) = send.ended else {
            return;
        };
        let closed = self.window_closed;
        if failed && closed {
            info!("a send failed with the window closed; reopening it");
            self.reopen(out);
        }
        if self.section != Section::Send || closed {
            let (kind, text) = ended_toast(failed, line);
            self.toast(kind, text);
        }
    }
}
