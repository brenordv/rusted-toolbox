//! The message list: a virtualized tail-following list over the Read rows (or the quick
//! search's matches), with a pill that brings the newest rows back into view.

use crate::app::dispatch;
use crate::design::parts;
use crate::design::tokens::{
    COLUMN_ENQUEUED, COLUMN_PARTITION, COLUMN_SEQUENCE, COLUMN_SIZE, EDGE_FADE, LIST_HEADER_HEIGHT,
    LIST_ROW_HEIGHT, SELECTION_INSET, SPACE_LG,
};
use crate::read::inspector::FeedbackKind;
use crate::read::rows::{RowSource, RowSummary};
use crate::read::{ReadInput, ReadState, Step};
use crate::state::{AppState, Input};
use common_utils::string_utils::{format_bytes_to_string, format_thousands};
use rune::{El, GrowthSpec, Key, KeyEvent, ScrollSpec, Tx, col, row};
use rune_kit::{Kit, ScrollBar};
use std::borrow::Cow;

/// The list's scroll viewport key; scroll requests name it.
pub const LIST_KEY: &str = "read-list";
/// The focusable list element's key; focus moves to it from the quick search.
pub const MESSAGE_LIST_KEY: &str = "message-list";

/// The rows a list shows, found in the app state.
pub type Source<R> = fn(&AppState) -> &R;

/// The Read section's rows.
pub fn read_rows(s: &AppState) -> &ReadState {
    &s.read
}

pub fn list<R: RowSource + 'static>(s: &AppState, k: &Kit, dark: bool, source: Source<R>) -> El {
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

    let rows = col()
        .key("read-rows")
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows(
            source(s).count(),
            LIST_ROW_HEIGHT,
            move |s: &AppState, i| match source(s).row(i) {
                Some(row) => Cow::Borrowed(row.key_str.as_str()),
                None => Cow::Owned(format!("gone-{i}")),
            },
            move |s: &AppState, i| list_row(s, dark, source(s).row(i)),
        )
        .virtual_rows_version(source(s).epoch())
        .on_visible_rows(move |s: &mut AppState, range| {
            let count = source(s).count();
            s.read.visible_rows(range.end, count)
        });
    let viewport = col()
        .key(LIST_KEY)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: true })
        .scroll_edge_fade(EDGE_FADE)
        .child(rows)
        .scroll_bar(k);

    let mut list = col()
        .key(MESSAGE_LIST_KEY)
        .role("list")
        .label("Messages")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .focusable(true)
        .on_key(list_key)
        .child(header)
        .child(parts::hairline(k))
        .child(viewport);
    if s.read.tail.pill_shows() {
        let label = format!("↓ {} new", format_thousands(s.read.tail.shown_unseen));
        list = list.child(
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
                ),
        );
    }
    list
}

fn list_row(s: &AppState, dark: bool, summary: Option<&RowSummary>) -> El {
    let k = s.ds.kit(dark);
    let Some(summary) = summary else {
        return row().h(LIST_ROW_HEIGHT);
    };
    let message = &summary.message;
    let selected = s
        .read
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
            dispatch(s, Input::Read(ReadInput::Select(key_str.clone())));
        })
}

/// Keys while the list has focus. `Key` gains variants over time, so keys the list does not
/// use are declined and go on to the window's handler.
fn list_key(s: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if key.is_command('c') {
        return copy_selected_body(s, tx);
    }
    let input = match key.key {
        Key::Up => ReadInput::Move(Step::Up),
        Key::Down => ReadInput::Move(Step::Down),
        Key::Home => ReadInput::Move(Step::First),
        Key::End => ReadInput::Move(Step::Last),
        Key::Escape if s.read.selected.is_some() => ReadInput::ClearSelection,
        _ => return false,
    };
    dispatch(s, Input::Read(input));
    if let Some(selected) = &s.read.selected {
        tx.scroll_into_view_in(LIST_KEY, selected.key_str.clone());
    }
    true
}

/// Copies the selected message's whole body; false when nothing is selected.
pub fn copy_selected_body(s: &mut AppState, tx: &mut Tx) -> bool {
    let Some(selected) = &s.read.selected else {
        return false;
    };
    tx.copy(selected.message.body.clone());
    dispatch(
        s,
        Input::Read(ReadInput::Copied {
            kind: FeedbackKind::CopiedBody,
            now: chrono::Utc::now(),
        }),
    );
    true
}
