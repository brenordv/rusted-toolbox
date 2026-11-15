//! The Export section: the archive search's results in the shared list, their footer, and the
//! floating bar that exports every match.

mod bar;
pub mod sidebar;

use super::message_list;
use crate::app::dispatch;
use crate::archive::ArchiveInput;
use crate::archive::results::{More, Results};
use crate::design::tokens::{SPACE_LG, SPACE_SM};
use crate::design::{icons, parts};
use crate::state::{AppState, Input, ListSection};
use common_utils::string_utils::format_thousands;
use rune::{El, col, row, spacer};
use rune_kit::{Button, ButtonVariant, Kit, Spinner};

pub fn content(s: &AppState, k: &Kit, dark: bool, layer: &mut Vec<El>) -> El {
    let archive = &s.archive;
    let area = match &archive.results {
        Results::NotSearched => parts::empty_state(
            k,
            "export-empty",
            &icons::ARCHIVE,
            "Search the archive",
            "Pick a time window or text and press Search.",
            None,
        ),
        Results::Loading { .. } => parts::empty_state_with_note(
            k,
            "export-searching",
            &icons::ARCHIVE,
            "Searching the archive",
            "Matching messages appear here.",
            Some(Spinner::new().build(k)),
            None,
        ),
        Results::Unavailable { error, .. } => parts::empty_state(
            k,
            "export-unavailable",
            &icons::ARCHIVE,
            "Couldn't search the archive",
            error,
            None,
        ),
        Results::Shown { rows, more, .. } if rows.is_empty() => match more {
            More::End | More::Capped => parts::empty_state(
                k,
                "export-no-matches",
                &icons::ARCHIVE,
                "No matches",
                "Nothing in the archive matches this search.",
                None,
            ),
            More::Ready(_) | More::Loading(_) | More::Failed { .. } => {
                results(s, k, dark, rows.len(), more)
            }
        },
        Results::Shown { rows, more, .. } => results(s, k, dark, rows.len(), more),
    };
    let mut content = col()
        .key("export-content")
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(area);
    if let Some(bar) = bar::bar(s, k, layer) {
        content = content.child(
            row()
                .pin(Some(0.0), None, Some(0.0), Some(SPACE_LG))
                .center()
                .child(bar),
        );
    }
    content
}

fn results(s: &AppState, k: &Kit, dark: bool, rows: usize, more: &More) -> El {
    let count = format_thousands(rows as u64);
    let line = match more {
        More::End => format!("{count} matches"),
        More::Ready(_) | More::Loading(_) | More::Failed { .. } | More::Capped => {
            format!("{count} loaded")
        }
    };
    let toolbar = row()
        .key("export-toolbar")
        .w_full()
        .center_y()
        .px(SPACE_LG)
        .py(SPACE_SM)
        .child(parts::live_numbers(k, &line))
        .child(spacer());
    col()
        .key("export-results")
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(toolbar)
        .child(parts::hairline(k))
        .child(message_list::list(
            s,
            k,
            dark,
            ListSection::Export,
            footer(k, rows, more),
        ))
}

/// What follows the rows: a spinner while the next page loads, its error with Retry, or the
/// cap's note.
fn footer(k: &Kit, count: usize, more: &More) -> Option<El> {
    let line = row()
        .key("results-footer")
        .w_full()
        .gap(SPACE_SM)
        .center()
        .px(SPACE_LG)
        .py(SPACE_LG);
    match more {
        More::Loading(_) => Some(line.child(Spinner::new().build(k))),
        More::Failed { .. } => Some(
            line.child(parts::caption(k, "Couldn't load more results"))
                .child(
                    Button::new("Retry")
                        .variant(ButtonVariant::Ghost)
                        .on_press(|s: &mut AppState| {
                            dispatch(s, Input::Archive(ArchiveInput::RetryPage));
                        })
                        .build(k)
                        .key("retry-page"),
                ),
        ),
        More::Capped => Some(line.child(parts::caption(
            k,
            &format!(
                "Showing the first {} matches. Narrow the search or export them all.",
                format_thousands(count as u64)
            ),
        ))),
        More::Ready(_) | More::End => None,
    }
}
