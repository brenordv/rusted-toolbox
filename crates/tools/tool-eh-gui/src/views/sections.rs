//! The Read and Export content areas and the sidebar. While a session captures, the Read area
//! is the live list; otherwise each area shows the empty state its connection state calls for.
//! The still-finishing notice takes the Read area's place whatever the connection state.

use super::{details, export, read, send};
use crate::app::dispatch;
use crate::design::{icons, parts};
use crate::state::{
    AppState, ConnectionState, Failure, FailureStage, Input, Job, Section, SheetPurpose,
    error_preview, still_finishing_text,
};
use rune::{El, Tx};
use rune_kit::{Button, ButtonVariant, Kit};

/// Connect, disabled while a send runs (a running send keeps every sheet closed).
fn connect_button(s: &AppState, k: &Kit) -> El {
    Button::new("Connect")
        .variant(ButtonVariant::Submit)
        .disabled(s.send.is_running())
        .on_press(|s: &mut AppState| dispatch(s, Input::OpenSheet(SheetPurpose::Connect)))
        .build(k)
        .key("empty-connect")
}

/// The notice of a window that reopened because `waited` ran past the close's bound, with
/// Force quit.
fn still_finishing(k: &Kit, waited: &[Job]) -> El {
    parts::empty_state(
        k,
        "still-finishing",
        &icons::ARCHIVE,
        "Still finishing",
        &still_finishing_text(waited),
        Some(
            Button::new("Force quit")
                .variant(ButtonVariant::Danger)
                .on_press(|s: &mut AppState| dispatch(s, Input::ForceQuit))
                .build(k)
                .key("force-quit"),
        ),
    )
}

pub fn read(s: &AppState, k: &Kit, dark: bool, layer: &mut Vec<El>) -> El {
    if let Some(waited) = s.waited() {
        return still_finishing(k, waited);
    }
    match &s.connection {
        ConnectionState::Disconnected => parts::empty_state(
            k,
            "not-connected",
            &icons::ANTENNA,
            "Not connected",
            "Connect to an Event Hub to start capturing.",
            Some(connect_button(s, k)),
        ),
        ConnectionState::Connecting { .. } => parts::empty_state(
            k,
            "connecting",
            &icons::ANTENNA,
            "Connecting",
            "Opening the archive and signing in to the event hub.",
            None,
        ),
        ConnectionState::Connected { partitions, .. } => {
            read::content(s, k, dark, partitions.len(), layer)
        }
        ConnectionState::Stopping { .. } => parts::empty_state(
            k,
            "finishing",
            &icons::ARCHIVE,
            "Finishing the session",
            "Writing the last message files.",
            None,
        ),
        ConnectionState::Failed(failure) => failed(s, k, failure),
    }
}

/// The empty state of a failed connection: the failure cut short, with its details when cut,
/// and Connect.
fn failed(s: &AppState, k: &Kit, failure: &Failure) -> El {
    let preview = error_preview(&failure.text);
    let note = preview.cut.then(|| {
        details::button(k, details::FAILURE_KEY, |s: &mut AppState, _: &mut Tx| {
            details::open_failure(s);
        })
    });
    let title = details::failure_title(failure.stage);
    match failure.stage {
        FailureStage::Starting => parts::empty_state_with_note(
            k,
            "failed",
            &icons::ANTENNA,
            title,
            &preview.shown,
            note,
            Some(connect_button(s, k)),
        ),
        FailureStage::Run => parts::empty_state_with_note(
            k,
            "session-failed",
            &icons::ARCHIVE,
            title,
            &format!(
                "{} Captured messages are safe in the archive; eh export or the Export \
                 section can write what is missing.",
                preview.shown
            ),
            note,
            Some(connect_button(s, k)),
        ),
    }
}

pub fn sidebar(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> El {
    match s.section {
        Section::Read => read::rules::sidebar(s, k, layer),
        Section::Export => export::sidebar::sidebar(s, k, layer),
        Section::Send => send::sidebar::sidebar(s, k, layer),
    }
}
