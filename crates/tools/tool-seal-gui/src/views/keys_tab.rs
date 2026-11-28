//! The Keys tab: the recipient book with its add form, the key pair the user generates, what
//! the log file is doing, and the action row with the status line.

use super::{standing_captions, tab_status, window_key};
use crate::design::{BOOK_MIN_ROWS, FORM_MAX_WIDTH, LABEL_FIELD_WIDTH};
use crate::services::dispatch;
use crate::state::keys::{KeygenStep, KeysInput};
use crate::state::recipients::{BookEntry, truncated_key};
use crate::state::{AppState, Input, Tab};
use common_cli::app_logger::FileLogging;
use common_gui_rune::keys::is_chord;
use common_gui_rune::parts::{
    LineFieldSpec, ListHeight, ListRows, StatusLines, Tone, action_row, caption, detail_row,
    line_field, list_box, reveal, section_header, status_line, warning,
};
use common_gui_rune::tokens::{
    CONTROL_HEIGHT, HAIRLINE, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XS, TYPE_BODY, TYPE_LABEL,
    TYPE_MONO,
};
use common_utils::string_utils::display_line;
use rune::{Cx, El, KeyEvent, ScrollSpec, Tx, col, row, spacer, text};
use rune_kit::{Button, ButtonVariant, Kit, ScrollBar};

/// The Keys tab's scroll viewport.
pub const KEYS_SCROLL_KEY: &str = "seal-keys-scroll";
/// What the scroll viewport holds: the form with its insets.
pub const KEYS_FORM_KEY: &str = "seal-keys-form";
/// The recipient book.
pub const BOOK_KEY: &str = "seal-keys-book";
/// The add form's Label field.
pub const LABEL_KEY: &str = "seal-keys-label";
/// The add form's Public key field.
pub const PUBLIC_KEY_KEY: &str = "seal-keys-public-key";
/// "Add recipient".
pub const ADD_RECIPIENT_KEY: &str = "seal-keys-add";
/// "Generate key pair".
pub const GENERATE_KEY: &str = "seal-keys-generate";
/// The replace confirm's line.
pub const CONFIRM_KEY: &str = "seal-keys-confirm";
/// "Replace key file".
pub const REPLACE_KEY: &str = "seal-keys-replace";
/// The replace confirm's "Cancel".
pub const CANCEL_KEY: &str = "seal-keys-cancel";
/// The generated public key.
pub const GENERATED_KEY: &str = "seal-keys-generated";
/// "Copy" on the generated key.
pub const COPY_GENERATED_KEY: &str = "seal-keys-copy-generated";
/// "Add to recipient book".
pub const ADD_GENERATED_KEY: &str = "seal-keys-add-generated";
/// The About group's log line.
pub const LOG_FOLDER_KEY: &str = "seal-keys-log";
/// The action row.
pub const KEYS_ACTIONS_KEY: &str = "seal-keys-actions";
/// The action row's status line.
pub const KEYS_STATUS_KEY: &str = "seal-keys-status";

/// The key of the book row of `entry`.
pub fn book_row_key(entry: &BookEntry) -> String {
    format!("seal-keys-row-{}", entry.id.number())
}

/// The key of "Copy" on the book row with this id number.
pub fn copy_entry_key(number: u64) -> String {
    format!("seal-keys-copy-{number}")
}

/// The key of "Delete" (or "Confirm delete") on the book row with this id number.
pub fn delete_key(number: u64) -> String {
    format!("seal-keys-delete-{number}")
}

/// The Keys tab.
pub fn view(s: &AppState, k: &Kit, cx: &mut Cx<AppState>) -> El {
    let viewport = cx.layout_with(KEYS_SCROLL_KEY, |rect| rect[3]);
    let confirm_kit = s.confirm_kits.kit(s.theme.dark(cx.system_dark()));
    let form = col()
        .w_full()
        .max_w(FORM_MAX_WIDTH)
        .grow(1.0)
        .child(book(s, (k, confirm_kit)))
        .child(key_pair(s, k))
        .child(about(s, k));
    let body = col()
        .key(KEYS_SCROLL_KEY)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            col()
                .key(KEYS_FORM_KEY)
                .w_full()
                .min_h(viewport.unwrap_or(0.0))
                .padding(SPACE_LG)
                .items_center()
                .child(form),
        )
        .scroll_bar(k);
    let mut tab = col().w_full().grow(1.0).min_h(0.0).child(body);
    if let Some(captions) = standing_captions(s, k) {
        tab = tab.child(captions);
    }
    tab.child(action_row(
        k,
        KEYS_ACTIONS_KEY,
        tab_status(s, k, Tab::Keys, KEYS_STATUS_KEY),
        Vec::new(),
    ))
}

/// Keys in the add form's fields, before the field sees them: the window's shortcuts, then
/// Return and `mod+enter`, which add.
fn form_key(s: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if window_key(s, key, tx) {
        return true;
    }
    if is_chord("enter", key, tx) || is_chord("mod+enter", key, tx) {
        dispatch(s, tx, Input::Keys(KeysInput::AddPressed));
        return true;
    }
    false
}

/// The book, which fills the space the tab leaves, and the add form under it; `confirm_kit`
/// draws the armed row's "Confirm delete".
fn book(s: &AppState, (k, confirm_kit): (&Kit, &Kit)) -> El {
    let rows = s
        .book
        .entries()
        .iter()
        .map(|entry| {
            book_row(
                k,
                entry,
                (s.keys.armed == Some(entry.id)).then_some(confirm_kit),
            )
        })
        .collect();
    let issue = s
        .keys
        .issue
        .as_ref()
        .map(|issue| (issue.on_label(), issue.words()));
    let label_issue = issue
        .as_ref()
        .and_then(|(on_label, words)| on_label.then_some(words.as_str()));
    let key_issue = issue
        .as_ref()
        .and_then(|(on_label, words)| (!on_label).then_some(words.as_str()));
    let add = Button::new("Add recipient")
        .variant(ButtonVariant::Neutral)
        .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
            dispatch(s, tx, Input::Keys(KeysInput::AddPressed));
        })
        .build(k)
        .key(ADD_RECIPIENT_KEY);
    let form = row()
        .w_full()
        .mt(SPACE_MD)
        .gap(SPACE_MD)
        .child(col().w(LABEL_FIELD_WIDTH).child(line_field(
            k,
            LineFieldSpec {
                key: LABEL_KEY,
                label: "Label",
                placeholder: "",
                issue: label_issue,
            },
            &s.keys.label,
            |s: &mut AppState| &mut s.keys.label,
            form_key,
            |s: &mut AppState, tx: &mut Tx| {
                dispatch(s, tx, Input::Keys(KeysInput::LabelEdited));
            },
        )))
        .child(col().grow(1.0).min_w(0.0).child(line_field(
            k,
            LineFieldSpec {
                key: PUBLIC_KEY_KEY,
                label: "Public key",
                placeholder: "age1…",
                issue: key_issue,
            },
            &s.keys.key,
            |s: &mut AppState| &mut s.keys.key,
            form_key,
            |s: &mut AppState, tx: &mut Tx| {
                dispatch(s, tx, Input::Keys(KeysInput::KeyEdited));
            },
        )))
        // The button lines up with the fields, under their labels.
        .child(add.mt(TYPE_LABEL.line_px() + SPACE_XS));
    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(section_header(k, "Recipient book").mt(0.0).mb(SPACE_SM))
        .child(list_box(
            k,
            (BOOK_KEY, "Recipient book"),
            ListRows::Built(rows),
            "No saved recipients yet.",
            ListHeight::Fill {
                min_rows: BOOK_MIN_ROWS,
            },
        ))
        .child(form)
}

/// One saved recipient: the label, the shortened key (described by the whole key), Copy, and
/// Delete, which reads "Confirm delete" in the danger look, drawn with `armed`'s kit, once
/// pressed.
fn book_row(k: &Kit, entry: &BookEntry, armed: Option<&Kit>) -> El {
    let id = entry.id;
    let label = display_line(entry.label.chars());
    let copy = Button::new("Copy")
        .variant(ButtonVariant::Ghost)
        .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
            dispatch(s, tx, Input::Keys(KeysInput::CopyEntry(id)));
        })
        .build(k)
        .key(copy_entry_key(id.number()))
        .label(format!("Copy {label}"));
    let press_delete = move |s: &mut AppState, tx: &mut Tx| {
        dispatch(s, tx, Input::Keys(KeysInput::DeletePressed(id)));
    };
    let delete = if let Some(confirm_kit) = armed {
        Button::new("Confirm delete")
            .variant(ButtonVariant::Danger)
            .on_press_tx(press_delete)
            .build(confirm_kit)
            .border(HAIRLINE, k.c.danger.fg)
            .key(delete_key(id.number()))
            .label(format!("Confirm delete {label}"))
    } else {
        Button::new("Delete")
            .variant(ButtonVariant::Ghost)
            .on_press_tx(press_delete)
            .build(k)
            .key(delete_key(id.number()))
            .label(format!("Delete {label}"))
    };
    row()
        .key(book_row_key(entry))
        .w_full()
        .min_h(CONTROL_HEIGHT)
        .gap(SPACE_SM)
        .center_y()
        .child(
            text(label.clone())
                .font(TYPE_BODY)
                .color(k.c.text_base)
                .truncate()
                .min_w(0.0)
                .shrink(),
        )
        .child(spacer())
        .child(
            text(truncated_key(&entry.public_key))
                .font(TYPE_MONO)
                .color(k.c.text_muted)
                .description(entry.public_key.clone()),
        )
        .child(copy)
        .child(delete)
}

/// "Your key pair": Generate, the replace confirm while it waits, and the generated key.
fn key_pair(s: &AppState, k: &Kit) -> El {
    let running = s.running_on(Tab::Keys);
    let confirming = match &s.keys.step {
        KeygenStep::Idle => None,
        KeygenStep::ConfirmReplace { path } => Some(path),
    };
    let generate = Button::new("Generate key pair")
        .variant(if running {
            ButtonVariant::Loading
        } else {
            ButtonVariant::Neutral
        })
        .disabled(!running && (s.blocked().is_some() || s.dialog.is_some() || confirming.is_some()))
        .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
            dispatch(s, tx, Input::Keys(KeysInput::GeneratePressed));
        })
        .build(k)
        .key(GENERATE_KEY);
    let mut group = col()
        .w_full()
        .child(section_header(k, "Your key pair").mb(SPACE_SM))
        .child(row().child(generate));
    if let Some(path) = confirming {
        let file = display_line(path.display().to_string().chars());
        let replace = Button::new("Replace key file")
            .variant(ButtonVariant::Outline)
            .disabled(s.blocked().is_some())
            .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                dispatch(s, tx, Input::Keys(KeysInput::ReplacePressed));
            })
            .build(k)
            .key(REPLACE_KEY);
        let cancel = Button::new("Cancel")
            .variant(ButtonVariant::Ghost)
            .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                dispatch(s, tx, Input::Keys(KeysInput::CancelPressed));
            })
            .build(k)
            .key(CANCEL_KEY);
        group = group.child(
            reveal(
                col()
                    .w_full()
                    .mt(SPACE_MD)
                    .gap(SPACE_SM)
                    .child(status_line(
                        k,
                        CONFIRM_KEY,
                        Tone::Warning,
                        StatusLines::Wrap,
                        &format!("{file} already exists. Replacing it destroys the key in it."),
                    ))
                    .child(row().gap(SPACE_SM).child(replace).child(cancel)),
            )
            .key("seal-keys-confirm-group"),
        );
    }
    if let Some(generated) = &s.keys.generated {
        let in_book = s
            .book
            .entries()
            .iter()
            .any(|entry| entry.public_key == generated.public_key);
        let copy = Button::new("Copy")
            .variant(ButtonVariant::Ghost)
            .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                dispatch(s, tx, Input::Keys(KeysInput::CopyGenerated));
            })
            .build(k)
            .key(COPY_GENERATED_KEY)
            .label("Copy the public key");
        let add = Button::new("Add to recipient book")
            .variant(ButtonVariant::Ghost)
            .disabled(in_book)
            .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                dispatch(s, tx, Input::Keys(KeysInput::AddGeneratedPressed));
            })
            .build(k)
            .key(ADD_GENERATED_KEY);
        group = group.child(
            row()
                .w_full()
                .mt(SPACE_MD)
                .gap(SPACE_SM)
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .key(GENERATED_KEY)
                        .child(detail_row(k, "Public key", &generated.public_key)),
                )
                .child(copy)
                .child(add),
        );
    }
    group
}

/// "About": where the log file is, or why there is none.
fn about(s: &AppState, k: &Kit) -> El {
    let line = match &s.file_logging {
        FileLogging::Writing(path) => {
            let folder = path.parent().unwrap_or(path.as_path());
            detail_row(
                k,
                "Log folder",
                &display_line(folder.display().to_string().chars()),
            )
        }
        FileLogging::Off => caption(k, "File logging is off."),
        FileLogging::Failed { error, .. } => warning(
            k,
            &format!(
                "The log file could not be opened: {}",
                display_line(error.chars())
            ),
        ),
    };
    col()
        .w_full()
        .child(section_header(k, "About").mb(SPACE_SM))
        .child(col().w_full().key(LOG_FOLDER_KEY).child(line))
}
