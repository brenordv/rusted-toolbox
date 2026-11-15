//! The Read and Export content areas and the sidebar. While a session captures, the Read area
//! is the live list; otherwise each area shows the empty state its connection state calls for.

use super::{export, read};
use crate::app::dispatch;
use crate::design::{icons, parts};
use crate::state::{AppState, ConnectionState, FailureStage, Input, Section};
use rune::El;
use rune_kit::{Button, ButtonVariant, Kit};

fn connect_button(k: &Kit) -> El {
    Button::new("Connect")
        .variant(ButtonVariant::Submit)
        .on_press(|s: &mut AppState| dispatch(s, Input::OpenSheet))
        .build(k)
        .key("empty-connect")
}

pub fn read(s: &AppState, k: &Kit, dark: bool, layer: &mut Vec<El>) -> El {
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
        ConnectionState::Connected { partitions, .. } => {
            read::content(s, k, dark, partitions.len(), layer)
        }
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

pub fn sidebar(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> El {
    match s.section {
        Section::Read => read::rules::sidebar(s, k, layer),
        Section::Export => export::sidebar::sidebar(s, k, layer),
    }
}
