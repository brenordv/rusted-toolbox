//! The message list both sections share: a virtualized list over a section's rows. The Read
//! list follows the tail and has a pill that brings the newest rows back into view; the Export
//! list is a plain scroll over search results with a footer after its rows.

use super::inspector::inspector_of;
use crate::app::{dispatch, run_effects};
use crate::archive::ArchiveInput;
use crate::design::parts;
use crate::design::tokens::{
    COLUMN_ENQUEUED, COLUMN_PARTITION, COLUMN_SEQUENCE, COLUMN_SIZE, EDGE_FADE, LIST_HEADER_HEIGHT,
    LIST_ROW_HEIGHT, SELECTION_INSET, SPACE_LG,
};
use crate::read::inspector::{FeedbackKind, InspectorInput};
use crate::read::rows::{RowSource, RowSummary};
use crate::read::{ReadInput, Step};
use crate::state::{AppState, Input, Section};
use common_utils::string_utils::{format_bytes_to_string, format_thousands};
use rune::{El, GrowthSpec, Key, KeyEvent, ScrollSpec, Tx, col, row};
use rune_kit::{Kit, ScrollBar};
use std::borrow::Cow;
use std::ops::Range;

/// The Read list's scroll viewport key; scroll requests name it.
pub const LIST_KEY: &str = "read-list";
/// The Read list's focusable element; focus moves to it from the quick search.
pub const MESSAGE_LIST_KEY: &str = "message-list";
/// The Export list's scroll viewport key.
pub const ARCHIVE_LIST_KEY: &str = "archive-list";
/// The Export list's focusable element.
pub const ARCHIVE_MESSAGE_LIST_KEY: &str = "archive-message-list";

/// The rows `section` shows.
fn rows_of(s: &AppState, section: Section) -> &dyn RowSource {
    match section {
        Section::Read => &s.read,
        Section::Export => &s.archive,
    }
}

/// The scroll viewport key of `section`'s list.
pub fn scroll_key(section: Section) -> &'static str {
    match section {
        Section::Read => LIST_KEY,
        Section::Export => ARCHIVE_LIST_KEY,
    }
}

fn focus_key(section: Section) -> &'static str {
    match section {
        Section::Read => MESSAGE_LIST_KEY,
        Section::Export => ARCHIVE_MESSAGE_LIST_KEY,
    }
}

fn select_input(section: Section, key_str: String) -> Input {
    match section {
        Section::Read => Input::Read(ReadInput::Select(key_str)),
        Section::Export => Input::Archive(ArchiveInput::Select(key_str)),
    }
}

fn move_input(section: Section, step: Step) -> Input {
    match section {
        Section::Read => Input::Read(ReadInput::Move(step)),
        Section::Export => Input::Archive(ArchiveInput::Move(step)),
    }
}

/// The input that clears `section`'s selection.
pub fn clear_selection_input(section: Section) -> Input {
    match section {
        Section::Read => Input::Read(ReadInput::ClearSelection),
        Section::Export => Input::Archive(ArchiveInput::ClearSelection),
    }
}

/// The list's visible rows changed; returns whether anything shown changed.
fn visible_rows(s: &mut AppState, section: Section, range: Range<usize>) -> bool {
    match section {
        Section::Read => {
            let count = RowSource::count(&s.read);
            s.read.visible_rows(range.end, count)
        }
        Section::Export => {
            let outcome = s.apply(Input::Archive(ArchiveInput::VisibleRows { end: range.end }));
            run_effects(s, outcome.effects);
            outcome.changed
        }
    }
}

/// `section`'s list, with `footer` after its rows.
pub fn list(s: &AppState, k: &Kit, dark: bool, section: Section, footer: Option<El>) -> El {
    let header = row()
        .key("list-header")
        .w_full()
        .h(LIST_HEADER_HEIGHT)
        .center_y()
        .px(SELECTION_INSET)
        .child(parts::column_header(k, "Enqueued", Some(COLUMN_ENQUEUED)))
        .child(parts::column_header(k, "Partition", Some(COLUMN_PARTITION)))
        .child(parts::column_header(k, "Seq", Some(COLUMN_SEQUENCE)))
        .child(parts::column_header(k, "Size", Some(COLUMN_SIZE)))
        .child(parts::column_header(k, "Preview", None));

    let source = rows_of(s, section);
    let rows = col()
        .key(match section {
            Section::Read => "read-rows",
            Section::Export => "archive-rows",
        })
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows(
            source.count(),
            LIST_ROW_HEIGHT,
            move |s: &AppState, i| match rows_of(s, section).row(i) {
                Some(row) => Cow::Borrowed(row.key_str.as_str()),
                None => Cow::Owned(format!("gone-{i}")),
            },
            move |s: &AppState, i| list_row(s, dark, section, rows_of(s, section).row(i)),
        )
        .virtual_rows_version(source.epoch())
        .on_visible_rows(move |s: &mut AppState, range| visible_rows(s, section, range));
    let mut viewport = col()
        .key(scroll_key(section))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: section == Section::Read,
        })
        .scroll_edge_fade(EDGE_FADE)
        .child(rows);
    if let Some(footer) = footer {
        viewport = viewport.child(footer);
    }
    let viewport = viewport.scroll_bar(k);

    let mut list = col()
        .key(focus_key(section))
        .role("list")
        .label("Messages")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .focusable(true)
        .on_key(move |s: &mut AppState, key: &KeyEvent, tx: &mut Tx| list_key(s, key, tx, section))
        .child(header)
        .child(parts::hairline(k))
        .child(viewport);
    if section == Section::Read && s.read.tail.pill_shows() {
        list = list.child(new_messages_pill(s, k));
    }
    list
}

fn new_messages_pill(s: &AppState, k: &Kit) -> El {
    let label = format!("↓ {} new", format_thousands(s.read.tail.shown_unseen));
    row()
        .pin(Some(0.0), None, Some(0.0), Some(SPACE_LG))
        .center()
        .child(
            parts::floating_pill(k, "new-messages", &label)
                .role("button")
                .label(label.clone())
                .on_press_tx(|_: &mut AppState, tx: &mut Tx| {
                    tx.scroll_to_bottom(LIST_KEY);
                }),
        )
}

fn list_row(s: &AppState, dark: bool, section: Section, summary: Option<&RowSummary>) -> El {
    let k = s.ds.kit(dark);
    let Some(summary) = summary else {
        return row().h(LIST_ROW_HEIGHT);
    };
    let message = &summary.message;
    let selected = inspector_of(s, section)
        .selected
        .as_ref()
        .is_some_and(|selected| selected.key_str == summary.key_str);
    let enqueued = message
        .enqueued_time
        .map_or("--".to_owned(), |at| at.format("%H:%M:%S%.3f").to_string());
    let partition = message.key.partition_id.as_str();
    let sequence = message.key.sequence_number.to_string();
    let cells = vec![
        parts::list_cell(k, &enqueued, Some(COLUMN_ENQUEUED), selected),
        parts::list_cell(k, partition, Some(COLUMN_PARTITION), selected),
        parts::list_cell(k, &sequence, Some(COLUMN_SEQUENCE), selected),
        parts::list_cell(
            k,
            &format_bytes_to_string(message.body.len() as u64),
            Some(COLUMN_SIZE),
            selected,
        ),
        parts::list_cell(k, &summary.preview, None, selected),
    ];
    let key_str = summary.key_str.clone();
    parts::list_row(k, cells, selected)
        .role("listitem")
        .label(format!(
            "Partition {partition}, sequence {sequence}, enqueued {enqueued}"
        ))
        .on_press(move |s: &mut AppState| {
            dispatch(s, select_input(section, key_str.clone()));
        })
}

/// Keys while the list has focus. `Key` gains variants over time, so keys the list does not
/// use are declined and go on to the window's handler.
fn list_key(s: &mut AppState, key: &KeyEvent, tx: &mut Tx, section: Section) -> bool {
    if key.is_command('c') {
        return copy_selected_body(s, tx, section);
    }
    let has_selection = inspector_of(s, section).selected.is_some();
    let input = match key.key {
        Key::Up => move_input(section, Step::Up),
        Key::Down => move_input(section, Step::Down),
        Key::Home => move_input(section, Step::First),
        Key::End => move_input(section, Step::Last),
        Key::Escape if has_selection => clear_selection_input(section),
        _ => return false,
    };
    dispatch(s, input);
    if let Some(selected) = &inspector_of(s, section).selected {
        tx.scroll_into_view_in(scroll_key(section), selected.key_str.clone());
    }
    true
}

/// Copies `section`'s selected message's whole body; false when nothing is selected.
pub fn copy_selected_body(s: &mut AppState, tx: &mut Tx, section: Section) -> bool {
    let Some(selected) = &inspector_of(s, section).selected else {
        return false;
    };
    tx.copy(selected.message.body.clone());
    dispatch(
        s,
        Input::inspector(
            section,
            InspectorInput::Copied {
                kind: FeedbackKind::CopiedBody,
                now: chrono::Utc::now(),
            },
        ),
    );
    true
}
