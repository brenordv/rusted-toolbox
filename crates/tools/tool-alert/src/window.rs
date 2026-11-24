//! The alert window: an optional title, the message, an optional line about the sound, and
//! Close.

use crate::models::{AlertSpec, ClosedBy};
use common_gui_rune::motion::CONTROL_PAIR;
use common_gui_rune::parts::{StatusLines, Tone, caption, hairline, status_line};
use common_gui_rune::tokens::{
    CONTROL_HEIGHT, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XL, TYPE_BODY, TYPE_TITLE,
};
use common_gui_rune::{Surface, Theme, surface};
use common_utils::string_utils::{display_line, display_lines, error_preview};
use rune::{Cx, El, Key, KeyEvent, Mods, ScrollSpec, Transition, Tx, col, row, spacer, text};
use rune_kit::{Button, ButtonVariant, Kit, ScrollBar};
use std::time::{Duration, Instant};
use tracing::info;

/// The window's root: the dialog.
pub const ROOT_KEY: &str = "alert-dialog";
/// The title line.
pub const TITLE_KEY: &str = "alert-title";
/// The message's scroll viewport.
pub const BODY_KEY: &str = "alert-body";
/// The message's text inside the viewport.
pub const BODY_TEXT_KEY: &str = "alert-body-text";
/// The line saying the sound could not play.
pub const SOUND_ISSUE_KEY: &str = "alert-sound-issue";
/// The footer's caption.
pub const FOOTER_NOTE_KEY: &str = "alert-footer-note";
/// The Close button.
pub const CLOSE_KEY: &str = "alert-close";

/// The window title with no alert title, and the start of every alert's window title.
pub const OS_TITLE: &str = "alert";
/// The dialog's accessible name when the alert has no title.
pub const DEFAULT_DIALOG_NAME: &str = "Alert";
/// The footer's caption: where the window came from.
pub const FOOTER_NOTE: &str = "Sent with the alert command";

/// How long after the window is first on screen a key may close it. A window that takes
/// focus while the user types would otherwise vanish on their next Return.
pub const KEY_GUARD: Duration = Duration::from_secs(1);

/// Something that may close the alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseEvent {
    /// A pointer press, or an assistive client's click, on Close.
    Pointer,
    /// Close pressed from the keyboard: Return anywhere in the window, or Space on Close.
    ButtonKey,
    /// Escape.
    Escape,
    /// Any other key.
    OtherKey,
}

/// How `event` closes the alert, when it does. A pointer press closes at any time. A key
/// closes only as a fresh press (`repeat` is the OS repeat of a held key) at least
/// [`KEY_GUARD`] after the window was first on screen; `since_shown` is `None` until then.
pub fn closes(event: CloseEvent, repeat: bool, since_shown: Option<Duration>) -> Option<ClosedBy> {
    match event {
        CloseEvent::Pointer => Some(ClosedBy::Button),
        CloseEvent::ButtonKey | CloseEvent::Escape => {
            let guarded = match since_shown {
                Some(elapsed) => elapsed < KEY_GUARD,
                None => true,
            };
            if repeat || guarded {
                None
            } else {
                Some(ClosedBy::Key)
            }
        }
        CloseEvent::OtherKey => None,
    }
}

/// The window's title in the OS: "alert", or "alert: " and the alert's title on one line, cut
/// at 140 characters.
pub fn os_title(title: Option<&str>) -> String {
    match title {
        Some(title) => format!("{OS_TITLE}: {}", error_preview(title).shown),
        None => OS_TITLE.to_owned(),
    }
}

/// What the alert window shows and how it ended.
pub struct AlertState {
    theme: Theme,
    title: Option<String>,
    body: String,
    has_sound: bool,
    sound_issue: Option<String>,
    shown_at: Option<Instant>,
    closed_by: Option<ClosedBy>,
    start_sound: Option<Box<dyn FnOnce()>>,
}

impl AlertState {
    /// The alert `spec` describes, not yet on screen. `start_sound` runs once, when the window
    /// is first seen.
    pub fn new(spec: &AlertSpec, start_sound: Option<Box<dyn FnOnce()>>) -> Self {
        Self {
            theme: Theme::new(),
            title: spec
                .title
                .as_deref()
                .map(|title| display_line(title.chars())),
            body: display_lines(&spec.body),
            has_sound: spec.sound.is_some(),
            sound_issue: None,
            shown_at: None,
            closed_by: None,
            start_sound,
        }
    }

    /// An empty alert with no title, no message, and no sound.
    pub fn detached() -> Self {
        let empty = AlertSpec {
            title: None,
            body: String::new(),
            sound: None,
        };
        Self::new(&empty, None)
    }

    /// How the window was closed, once it was.
    pub fn closed_by(&self) -> Option<ClosedBy> {
        self.closed_by
    }

    /// When the window was first seen on screen.
    pub fn shown_at(&self) -> Option<Instant> {
        self.shown_at
    }

    /// Why the sound could not play, when it could not.
    pub fn sound_issue(&self) -> Option<&str> {
        self.sound_issue.as_deref()
    }

    /// Records that the window was first seen on screen at `now`, and starts the sound. Later
    /// calls change nothing.
    pub fn mark_shown(&mut self, now: Instant) {
        if self.shown_at.is_some() {
            return;
        }
        self.shown_at = Some(now);
        if let Some(start_sound) = self.start_sound.take() {
            start_sound();
        }
    }

    /// Applies `event` at `now`; true when it closed the alert. The first close is the one
    /// recorded.
    pub fn close(&mut self, event: CloseEvent, repeat: bool, now: Instant) -> bool {
        if self.closed_by.is_some() {
            return false;
        }
        let since_shown = self
            .shown_at
            .map(|shown_at| now.saturating_duration_since(shown_at));
        match closes(event, repeat, since_shown) {
            Some(closed_by) => {
                self.closed_by = Some(closed_by);
                true
            }
            None => false,
        }
    }

    /// Records the window's own close control, unless a close was already recorded, and
    /// returns how the alert was closed.
    pub fn window_closed(&mut self) -> ClosedBy {
        *self.closed_by.get_or_insert(ClosedBy::Window)
    }

    /// Sets what the sound attempt found; true when the view changed.
    pub fn set_sound_issue(&mut self, issue: Option<String>) -> bool {
        if self.sound_issue == issue {
            return false;
        }
        self.sound_issue = issue;
        true
    }
}

/// Marks the window as first seen on screen now: stamps the time, names the window after the
/// alert's title, puts focus on Close with no ring, and starts the sound. Always true: the
/// view changed.
pub fn shown(s: &mut AlertState, tx: &mut Tx) -> bool {
    s.mark_shown(Instant::now());
    tx.set_title(os_title(s.title.as_deref()));
    tx.focus(CLOSE_KEY, false);
    info!(
        has_title = s.title.is_some(),
        body_chars = s.body.chars().count(),
        has_sound = s.has_sound,
        "alert: shown"
    );
    true
}

/// Sets the appearance the window draws in (true: dark); true when it changed.
pub fn set_appearance(s: &mut AlertState, dark: bool) -> bool {
    s.theme.set(dark)
}

/// Runs as the window closes: records the window's own close control when nothing else
/// closed it, and logs how the alert ended.
pub fn on_close(s: &mut AlertState, _: &mut Tx) {
    let by = s.window_closed();
    let elapsed_ms = s
        .shown_at
        .map(|shown_at| u64::try_from(shown_at.elapsed().as_millis()).unwrap_or(u64::MAX));
    info!(elapsed_ms, by = by.as_str(), "alert: closed");
}

/// Keys no control took: Escape closes, under the key guard.
pub fn window_key(s: &mut AlertState, key: &KeyEvent, tx: &mut Tx) -> bool {
    let event = if key.key == Key::Escape && key.mods == Mods::default() {
        CloseEvent::Escape
    } else {
        CloseEvent::OtherKey
    };
    if !s.close(event, tx.key_repeat(), Instant::now()) {
        return false;
    }
    tx.close_window();
    true
}

/// Close's press. Rune presses the default button on Return from anywhere in the window and
/// a focused button on Space; `Tx::by_keys` tells those from a pointer press.
fn close_pressed(s: &mut AlertState, tx: &mut Tx) {
    let event = if tx.by_keys() {
        CloseEvent::ButtonKey
    } else {
        CloseEvent::Pointer
    };
    if s.close(event, tx.key_repeat(), Instant::now()) {
        tx.close_window();
    }
}

/// The window's root.
pub fn view(s: &AlertState, cx: &mut Cx<AlertState>) -> El {
    let k = s.theme.kit(cx.system_dark());
    let mut dialog = col()
        .key(ROOT_KEY)
        .role("dialog")
        .label(s.title.as_deref().unwrap_or(DEFAULT_DIALOG_NAME))
        .description(s.body.as_str())
        .w_full()
        .h_full()
        .padding(SPACE_XL)
        .fill(surface(k, Surface::Window));
    if let Some(title) = &s.title {
        dialog = dialog.child(
            text(title.as_str())
                .key(TITLE_KEY)
                .font(TYPE_TITLE)
                .color(k.c.text_base)
                .truncate()
                .mb(SPACE_SM),
        );
    }
    dialog = dialog.child(body(s, k));
    if let Some(issue) = &s.sound_issue {
        dialog = dialog.child(sound_issue_line(k, issue));
    }
    rune_kit::root(dialog.child(footer(k)), Vec::new())
}

/// The message in a viewport that takes the height the other rows leave, and scrolls when the
/// text is taller.
fn body(s: &AlertState, k: &Kit) -> El {
    col()
        .key(BODY_KEY)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            text(s.body.as_str())
                .key(BODY_TEXT_KEY)
                .font(TYPE_BODY)
                .color(k.c.text_base)
                .wrap_anywhere()
                .selectable(true),
        )
        .scroll_bar(k)
}

/// One line in the warning color saying why the sound did not play, fading in as a field's
/// validation message does.
fn sound_issue_line(k: &Kit, issue: &str) -> El {
    let message = format!("Couldn't play the sound: {issue}");
    status_line(
        k,
        SOUND_ISSUE_KEY,
        Tone::Warning,
        StatusLines::One,
        &message,
    )
    .mt(SPACE_MD)
    .transition(Transition::opacity().springs(CONTROL_PAIR))
}

/// A hairline over a row with the muted caption leading and Close trailing.
fn footer(k: &Kit) -> El {
    let close = Button::new("Close")
        .variant(ButtonVariant::Submit)
        .on_press_tx(close_pressed)
        .build(k)
        .key(CLOSE_KEY)
        .default_button();
    col()
        .w_full()
        .gap(SPACE_MD)
        .mt(SPACE_LG)
        .child(hairline(k))
        .child(
            row()
                .w_full()
                .h(CONTROL_HEIGHT)
                .center_y()
                .child(caption(k, FOOTER_NOTE).key(FOOTER_NOTE_KEY))
                .child(spacer())
                .child(close),
        )
}

#[cfg(test)]
mod tests;
