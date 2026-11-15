//! Toasts: the kit's stack and the one clock it runs on, repeats that ring the front card, the
//! cap, and the hold that keeps a card alive while a details modal opened from it is open.
//!
//! The stack's clock is one second per tick, so an info toast lasts six ticks and no handler
//! reads the wall clock for it. While the hold is on, new toasts wait in a queue and the clock
//! stops; the queue comes in, in order, once the modal closes.

use super::{AppState, Motion, Outcome, ToastKind};
use rune_kit::toast_stack::VISIBLE;
use rune_kit::{ToastEffect, ToastEvent, ToastStack};
use tracing::debug;

/// The most toasts in the stack; a newer one dismisses the oldest.
pub const MAX_TOASTS: usize = VISIBLE;
/// Clock seconds (ticks) an info toast stays; danger toasts stay until closed.
pub const INFO_TOAST_SECS: f64 = 6.0;

/// What a toast shows: its kind, its whole text, and the key of the ring its latest repeat
/// mounts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToastBody {
    pub kind: ToastKind,
    pub text: String,
    pub pulse: Option<u64>,
}

/// What the user does to the stack.
#[derive(Debug)]
pub enum ToastInput {
    /// A card's Close.
    Dismiss(u64),
    /// The pointer came onto the stack (`true`) or left it.
    Pointer(bool),
    /// Alt+T: fan the stack out.
    Hotkey,
    /// Escape: fold the stack.
    Escape,
}

/// A toast raised while the hold is on, shown once it ends.
#[derive(Clone, Debug, PartialEq, Eq)]
struct HeldToast {
    kind: ToastKind,
    text: String,
}

/// The app's toasts: the kit's stack, its clock, the ring counter, and the held queue.
#[derive(Debug, Default)]
pub(super) struct Toasts {
    stack: ToastStack<ToastBody>,
    /// The stack's clock, in seconds: one per tick that runs it.
    clock: f64,
    /// The last ring key handed out; only goes up, so every repeat mounts a new ring.
    next_pulse: u64,
    /// Oldest first; empty whenever the hold is off.
    held: Vec<HeldToast>,
}

/// Where the stack stood before an input, to tell whether a card arrived or rang.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ToastMark {
    front: Option<u64>,
    pulse: u64,
}

/// What one step of the stack reported.
#[derive(Default)]
struct Stepped {
    closed: bool,
    pulsed: Option<u64>,
}

/// The stack key that coalesces copies: the kind and the text, so an info and a danger toast
/// with the same text stay apart.
fn coalesce_key(kind: ToastKind, text: &str) -> String {
    let kind = match kind {
        ToastKind::Info => "Info",
        ToastKind::Danger => "Danger",
    };
    format!("{kind}\u{1f}{text}")
}

/// A commit that shows a toast moves, though no input may have caused it; one that asked to
/// land at once still does.
fn toast_motion(motion: Motion) -> Motion {
    match motion {
        Motion::Still | Motion::Animated => Motion::Animated,
        Motion::Instant => Motion::Instant,
    }
}

impl AppState {
    /// The toasts, oldest first.
    pub fn toasts(&self) -> &ToastStack<ToastBody> {
        &self.toasts.stack
    }

    /// The card the open details modal came from, while it is open: new toasts wait and the
    /// clock stops until it closes, so that card stays.
    pub fn toast_hold(&self) -> Option<u64> {
        self.details.as_ref().and_then(|details| details.toast)
    }

    /// How many toasts wait for the hold to end.
    pub fn held_toasts(&self) -> usize {
        self.toasts.held.len()
    }

    /// The stack's clock, in seconds.
    pub fn toast_clock(&self) -> f64 {
        self.toasts.clock
    }

    /// Shows a toast: a repeat of the front card's rings it, a copy further back moves to the
    /// front, and past [`MAX_TOASTS`] the oldest leave. While the hold is on it waits instead.
    pub fn toast(&mut self, kind: ToastKind, text: String) {
        if self.toast_hold().is_some() {
            debug!("a toast waits for the details to close");
            self.toasts.held.push(HeldToast { kind, text });
            return;
        }
        self.show_toast(kind, text);
    }

    fn show_toast(&mut self, kind: ToastKind, text: String) {
        let secs = match kind {
            ToastKind::Info => Some(INFO_TOAST_SECS),
            ToastKind::Danger => None,
        };
        let now = self.toasts.clock;
        let shown = self.step_toasts(ToastEvent::Shown {
            key: coalesce_key(kind, &text),
            options: ToastBody {
                kind,
                text,
                pulse: None,
            },
            secs,
            now,
        });
        while self.toasts.stack.entries().len() > MAX_TOASTS {
            let Some(oldest) = self.toasts.stack.entries().first().map(|entry| entry.id) else {
                break;
            };
            self.step_toasts(ToastEvent::Dismissed {
                id: Some(oldest),
                now,
            });
        }
        // The kit rings the front entry, which the cap never dismisses, so the lookup finds it.
        if let Some(id) = shown.pulsed {
            self.toasts.next_pulse += 1;
            let pulse = self.toasts.next_pulse;
            match self
                .toasts
                .stack
                .entries_mut()
                .iter_mut()
                .find(|entry| entry.id == id)
            {
                Some(entry) => entry.options.pulse = Some(pulse),
                None => debug!(id, "the toast that rang is gone"),
            }
        }
    }

    fn step_toasts(&mut self, event: ToastEvent<ToastBody>) -> Stepped {
        let mut stepped = Stepped::default();
        for effect in self.toasts.stack.step(event) {
            match effect {
                ToastEffect::Pulse(id) => stepped.pulsed = Some(id),
                ToastEffect::Closed(_) => stepped.closed = true,
                // The once-a-second tick already wakes the app.
                ToastEffect::WakeAt(_) => {}
            }
        }
        stepped
    }

    pub(super) fn toast_input(&mut self, input: ToastInput, out: &mut Outcome) {
        let before = self.toasts.stack.stack();
        let now = self.toasts.clock;
        let empty = self.toasts.stack.entries().is_empty();
        match input {
            ToastInput::Dismiss(id) => {
                let present = self.toasts.stack.entries().iter().any(|e| e.id == id);
                if !present || self.details.is_some() {
                    debug!(id, present, "toast close ignored");
                    out.changed = false;
                    return;
                }
                self.step_toasts(ToastEvent::Dismissed { id: Some(id), now });
                return;
            }
            ToastInput::Pointer(true) => {
                if empty || self.window_closed {
                    out.changed = false;
                    return;
                }
                self.step_toasts(ToastEvent::PointerOver { now });
            }
            // A modal from the stack would hand focus back to a card folded behind the front.
            ToastInput::Pointer(false) => {
                if self.details.is_some() {
                    out.changed = false;
                    return;
                }
                self.step_toasts(ToastEvent::PointerLeft { now });
            }
            ToastInput::Hotkey => {
                if empty || self.window_closed || self.details.is_some() {
                    out.changed = false;
                    return;
                }
                self.step_toasts(ToastEvent::Hotkey { now });
            }
            ToastInput::Escape => {
                self.step_toasts(ToastEvent::Escape { now });
            }
        }
        out.changed = self.toasts.stack.stack() != before;
    }

    /// Runs the stack's clock one second; true when a toast closed. The clock stands still
    /// while the hold is on.
    pub(super) fn tick_toasts(&mut self) -> bool {
        if self.toast_hold().is_some() {
            return false;
        }
        self.toasts.clock += 1.0;
        let now = self.toasts.clock;
        self.step_toasts(ToastEvent::Tick { now }).closed
    }

    /// Folds the stack as the pointer leaving it would; the pointer can't be over a closed
    /// window.
    pub(super) fn fold_toasts(&mut self) {
        let now = self.toasts.clock;
        self.step_toasts(ToastEvent::PointerLeft { now });
    }

    pub(super) fn toast_mark(&self) -> ToastMark {
        ToastMark {
            front: self.toasts.stack.front().map(|entry| entry.id),
            pulse: self.toasts.next_pulse,
        }
    }

    /// Shows what waited for the hold, once it is off; then, when a card arrived or rang since
    /// `before`, the commit changes and moves.
    pub(super) fn settle_toasts(&mut self, before: ToastMark, out: &mut Outcome) {
        if self.toast_hold().is_none() && !self.toasts.held.is_empty() {
            let held = std::mem::take(&mut self.toasts.held);
            debug!(count = held.len(), "showing the toasts the details held");
            for toast in held {
                self.show_toast(toast.kind, toast.text);
            }
        }
        if self.toast_mark() != before {
            out.changed = true;
            out.motion = toast_motion(out.motion);
        }
    }
}

#[cfg(test)]
mod tests;
