//! The Read and Export content areas and the sidebar. Capture and archive views fill these in
//! later phases; for now each shows the empty state its connection state calls for.

use crate::app::dispatch;
use crate::design::tokens::{SPACE_LG, SPACE_SM};
use crate::design::{icons, parts};
use crate::state::{AppState, ConnectionState, FailureStage, Input, Section, ShownStats};
use common_utils::string_utils::format_thousands;
use rune::{El, col};
use rune_kit::{Button, ButtonVariant, Kit};

fn connect_button(k: &Kit) -> El {
    Button::new("Connect")
        .variant(ButtonVariant::Submit)
        .on_press(|s: &mut AppState| dispatch(s, Input::OpenSheet))
        .build(k)
        .key("empty-connect")
}

/// "Received 12,403 · Stored 12,380".
fn counts(k: &Kit, shown: ShownStats) -> El {
    parts::live_numbers(
        k,
        &format!(
            "Received {} · Stored {}",
            format_thousands(shown.received),
            format_thousands(shown.stored)
        ),
    )
    .key("capture-counts")
}

pub fn read(s: &AppState, k: &Kit) -> El {
    match &s.connection {
        ConnectionState::Disconnected => parts::empty_state(
            k,
            "not-connected",
            &icons::ANTENNA,
            "Not connected",
            "Connect to an Event Hub to start capturing.",
            Some(connect_button(k)),
        ),
        ConnectionState::Connecting { .. } => parts::empty_state(
            k,
            "connecting",
            &icons::ANTENNA,
            "Connecting",
            "Opening the archive and signing in to the event hub.",
            None,
        ),
        ConnectionState::Connected {
            partitions, shown, ..
        } => parts::empty_state_with_note(
            k,
            "connected",
            &icons::ANTENNA,
            "Capturing",
            &format!(
                "Reading {} partitions into the archive in the data folder.",
                partitions.len()
            ),
            Some(counts(k, *shown)),
            None,
        ),
        ConnectionState::Stopping { .. } if s.still_finishing => parts::empty_state(
            k,
            "still-finishing",
            &icons::ARCHIVE,
            "Still finishing the session",
            "Captured messages are safe in the archive. The message files are still being \
             written; quitting now leaves the rest for eh export or the Export section.",
            Some(
                Button::new("Force quit")
                    .variant(ButtonVariant::Danger)
                    .on_press(|s: &mut AppState| dispatch(s, Input::ForceQuit))
                    .build(k)
                    .key("force-quit"),
            ),
        ),
        ConnectionState::Stopping { .. } => parts::empty_state(
            k,
            "finishing",
            &icons::ARCHIVE,
            "Finishing the session",
            "Writing the last message files.",
            None,
        ),
        ConnectionState::Failed(failure) => match failure.stage {
            FailureStage::Starting => parts::empty_state(
                k,
                "failed",
                &icons::ANTENNA,
                "Couldn't connect",
                &failure.text,
                Some(connect_button(k)),
            ),
            FailureStage::Run => parts::empty_state(
                k,
                "session-failed",
                &icons::ARCHIVE,
                "The session failed",
                &format!(
                    "{} Captured messages are safe in the archive; eh export or the Export \
                     section can write what is missing.",
                    failure.text
                ),
                Some(connect_button(k)),
            ),
        },
    }
}

pub fn export(k: &Kit) -> El {
    parts::empty_state(
        k,
        "export-empty",
        &icons::ARCHIVE,
        "Archive",
        "Searching and exporting the archive come in a later version. Until then, eh export \
         writes it from the command line.",
        None,
    )
}

pub fn sidebar(s: &AppState, k: &Kit) -> El {
    let (title, note) = match s.section {
        Section::Read => ("Capture rules", "Rules come in a later version."),
        Section::Export => ("Archive search", "Search comes in a later version."),
    };
    col()
        .gap(SPACE_SM)
        .px(SPACE_LG)
        .child(parts::section_header(k, title))
        .child(parts::caption(k, note))
}
