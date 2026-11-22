//! The error details modal, the "View details" button that opens it, and the openers for the
//! failures more than one surface shows.

use super::shell::return_toast_focus;
use crate::app::dispatch;
use crate::design::parts::{self, SheetFrame};
use crate::design::tokens::{DETAILS_WIDTH, SPACE_SM, SPACE_XL};
use crate::state::{AppState, ConnectionState, FailureStage, Input};
use chrono::Utc;
use rune::{El, ScrollSpec, Tx, col, row, spacer};
use rune_kit::{Button, ButtonVariant, Kit, ScrollBar};

/// The modal's key prefix: the panel is `details-dialog`.
pub const DETAILS_KEY: &str = "details";
/// The key of the Copy error button.
pub const COPY_KEY: &str = "details-copy";
/// The key of the Done button.
pub const DONE_KEY: &str = "details-done";
/// The key of the Read empty state's View details for the connection's failure.
pub const FAILURE_KEY: &str = "failure-details";

/// The error details modal while it is open: the whole text in a scroll view, Copy error, and
/// Done. Pushes the scrim and the sheet onto `layer`.
pub fn modal(s: &AppState, k: &Kit, layer: &mut Vec<El>) {
    let Some(details) = s.details() else {
        return;
    };
    let body = col()
        .key("details-scroll")
        .w_full()
        .shrink()
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            col()
                .key("details-text")
                .w_full()
                .px(SPACE_XL)
                .pt(SPACE_SM)
                .pb(SPACE_XL)
                .child(parts::body_text(k, &details.text)),
        )
        .scroll_bar(k);
    let copied = details.copied_until.is_some();
    let footer = row()
        .key("details-footer")
        .gap(SPACE_SM)
        .w_full()
        .center_y()
        .child(
            Button::new(if copied { "Copied" } else { "Copy error" })
                .variant(ButtonVariant::Neutral)
                .icon(if copied { k.glyph.check } else { k.glyph.copy })
                .on_press_tx(copy_details)
                .build(k)
                .key(COPY_KEY),
        )
        .child(spacer())
        .child(
            Button::new("Done")
                .variant(ButtonVariant::Submit)
                .on_press_tx(close)
                .build(k)
                .key(DONE_KEY)
                .default_button(),
        );
    parts::sheet(
        k,
        SheetFrame {
            key: DETAILS_KEY,
            title: &details.title,
            width: DETAILS_WIDTH,
        },
        close,
        body,
        footer,
        layer,
    );
}

/// Closes the modal. Toasts it held come in as it closes; when they folded or pushed out the
/// card it opened from, focus goes to the front card instead of back to that card.
fn close(s: &mut AppState, tx: &mut Tx) {
    let opener = s.toast_hold();
    dispatch(s, Input::CloseDetails);
    return_toast_focus(s, tx, opener);
}

/// Copies exactly what the modal shows; a plain copy, since failure text holds no credential.
fn copy_details(s: &mut AppState, tx: &mut Tx) {
    let Some(details) = s.details() else {
        return;
    };
    tx.copy(details.text.clone());
    dispatch(s, Input::DetailsCopied(Utc::now()));
}

/// A Ghost "View details" button keyed `key` that runs `open`.
pub fn button(k: &Kit, key: &str, open: impl Fn(&mut AppState, &mut Tx) + 'static) -> El {
    Button::new("View details")
        .variant(ButtonVariant::Ghost)
        .on_press_tx(open)
        .build(k)
        .key(key)
}

/// Opens the details modal on `text` under `title`.
pub fn show(s: &mut AppState, title: &str, text: &str) {
    dispatch(
        s,
        Input::ShowDetails {
            title: title.to_owned(),
            text: text.to_owned(),
            toast: None,
        },
    );
}

/// How the window names a failure of `stage`.
pub fn failure_title(stage: FailureStage) -> &'static str {
    match stage {
        FailureStage::Starting => "Couldn't connect",
        FailureStage::Run => "The session failed",
    }
}

/// Opens the details of the connection's failure; false (and nothing opens) when the connection
/// moved on.
pub fn open_failure(s: &mut AppState) -> bool {
    let (title, text) = match &s.connection {
        ConnectionState::Failed(failure) => (failure_title(failure.stage), failure.text.clone()),
        ConnectionState::Disconnected
        | ConnectionState::Connecting { .. }
        | ConnectionState::Connected { .. }
        | ConnectionState::Stopping { .. } => return false,
    };
    show(s, title, &text);
    true
}
