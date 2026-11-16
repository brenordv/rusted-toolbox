//! The Read section while a session captures: the stats strip, the list toolbar with the quick
//! search, the message list, and the inspector.

pub mod rules;
mod strip;

pub use strip::PARTITIONS_SCROLL_KEY;

pub(crate) use strip::short_duration;

use super::message_list;
use crate::app::dispatch;
use crate::design::tokens::{SEARCH_WIDTH, SPACE_LG, SPACE_SM};
use crate::design::{icons, parts};
use crate::read::ReadInput;
use crate::read::search::SearchState;
use crate::state::{AppState, Input, ListSection, error_preview};
use common_utils::string_utils::format_thousands;
use rune::{El, Key, Tx, col, row, spacer};
use rune_kit::{Button, ButtonVariant, Kit, TextInput};

/// The quick-search field's key; `mod+f` focuses it.
pub const SEARCH_KEY: &str = "quick-search";

pub fn content(s: &AppState, k: &Kit, dark: bool, partitions: usize, layer: &mut Vec<El>) -> El {
    let mut area = col()
        .key("read-live")
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(strip::strip(s, k, layer))
        .child(toolbar(s, k))
        .child(parts::hairline(k));
    if s.read.rows.dropped > 0 {
        area = area.child(
            parts::caption(
                k,
                &format!(
                    "Showing the latest {}. Older messages are in the archive.",
                    format_thousands(s.read.rows.displayed().len() as u64)
                ),
            )
            .px(SPACE_LG)
            .py(SPACE_SM),
        );
    }
    let waiting = s.read.rows.displayed().is_empty()
        && match s.read.search {
            SearchState::Off => true,
            SearchState::Active { .. } => false,
        };
    if waiting {
        return area.child(parts::empty_state(
            k,
            "waiting",
            &icons::ANTENNA,
            "Waiting for messages",
            &format!("Reading {partitions} partitions. New messages appear here as they arrive."),
            None,
        ));
    }
    area.child(message_list::list(s, k, dark, ListSection::Read, None))
}

fn toolbar(s: &AppState, k: &Kit) -> El {
    let read = &s.read;
    let regex = Button::new(".*")
        .variant(if read.search_regex {
            ButtonVariant::Outline
        } else {
            ButtonVariant::Ghost
        })
        .on_press(|s: &mut AppState| {
            s.read.search_regex = !s.read.search_regex;
            dispatch(s, Input::Read(ReadInput::SearchEdited));
        })
        .build(k)
        .key("search-regex")
        .label("Regular expression")
        .help("Regular expression");
    let field = TextInput::new(&read.search_edit)
        .leading_icon(k.glyph.search)
        .placeholder("Filter rows")
        .label("Filter rows")
        .width(Some(SEARCH_WIDTH))
        .invalid(read.search_error.is_some())
        .on_edit(|s: &mut AppState| &mut s.read.search_edit)
        .on_change(|s: &mut AppState, _: &mut Tx| {
            dispatch(s, Input::Read(ReadInput::SearchEdited));
        })
        .on_key(|s: &mut AppState, key, tx: &mut Tx| match key.key {
            Key::Escape => {
                dispatch(s, Input::Read(ReadInput::SearchCleared));
                true
            }
            Key::Down => {
                tx.focus(message_list::MESSAGE_LIST_KEY, true);
                true
            }
            // `Key` gains variants over time; other keys go on to the field and the window.
            _ => false,
        })
        .build(k)
        .key(SEARCH_KEY);
    let mut bar = row()
        .key("list-toolbar")
        .w_full()
        .gap(SPACE_SM)
        .center_y()
        .px(SPACE_LG)
        .py(SPACE_SM)
        .child(field)
        .child(regex);
    if let Some(error) = &read.search_error {
        bar = bar.child(parts::caption(k, &error_preview(error).shown));
    }
    bar = bar.child(spacer());
    match &read.search {
        SearchState::Off => {}
        SearchState::Active { visible, .. } => {
            bar = bar.child(parts::live_numbers(
                k,
                &format!(
                    "{} of {}",
                    format_thousands(visible.len() as u64),
                    format_thousands(read.rows.displayed().len() as u64)
                ),
            ));
        }
    }
    bar
}
