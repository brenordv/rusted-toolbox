//! The Recipients and Identity groups the Text and Files tabs share; each tab has its own keys
//! and its own recipient choice in state.

use super::parts::{ListHeight, list_box};
use super::window_key;
use super::{files_tab, text_tab};
use crate::services::dispatch;
use crate::state::files::FilesInput;
use crate::state::notes::Missing;
use crate::state::recipients::{BookEntry, EntryId, MSG_BAD_KEY, Picker, truncated_key};
use crate::state::text::TextInput;
use crate::state::{AppState, DialogPurpose, Input};
use common_gui_rune::parts::{LineFieldSpec, detail_row, line_field, section_header};
use common_gui_rune::tokens::{CONTROL_HEIGHT, SPACE_MD, SPACE_SM, TYPE_BODY, TYPE_MONO};
use common_utils::string_utils::display_line;
use rune::{El, TextEdit, Tx, col, row, spacer, text};
use rune_kit::{Button, ButtonVariant, Checkbox, Kit};

/// One picker's keys.
pub struct PickerKeys {
    /// The recipient list.
    pub recipients: &'static str,
    /// What each recipient checkbox's key starts with.
    pub recipient: &'static str,
    /// The Other key field.
    pub other_key: &'static str,
    /// The identity file row.
    pub identity: &'static str,
    /// "Choose identity file…".
    pub choose_identity: &'static str,
}

/// The keys of `picker`'s controls.
pub fn picker_keys(picker: Picker) -> PickerKeys {
    match picker {
        Picker::Text => PickerKeys {
            recipients: text_tab::RECIPIENTS_KEY,
            recipient: "seal-text-recipient",
            other_key: text_tab::OTHER_KEY_KEY,
            identity: text_tab::IDENTITY_KEY,
            choose_identity: text_tab::CHOOSE_IDENTITY_KEY,
        },
        Picker::Files => PickerKeys {
            recipients: files_tab::FILES_RECIPIENTS_KEY,
            recipient: "seal-files-recipient",
            other_key: files_tab::FILES_OTHER_KEY_KEY,
            identity: files_tab::FILES_IDENTITY_KEY,
            choose_identity: files_tab::FILES_CHOOSE_IDENTITY_KEY,
        },
    }
}

/// The key of the checkbox for the recipient `id` in `picker`.
pub fn recipient_key(picker: Picker, id: EntryId) -> String {
    format!("{}-{}", picker_keys(picker).recipient, id.number())
}

fn toggled(picker: Picker, id: EntryId) -> Input {
    match picker {
        Picker::Text => Input::Text(TextInput::RecipientToggled(id)),
        Picker::Files => Input::Files(FilesInput::RecipientToggled(id)),
    }
}

/// The recipient list, `rows` rows tall, and the Other key field; `choice_kit` draws the list's
/// checkboxes.
pub fn recipients(
    s: &AppState,
    (k, choice_kit): (&Kit, &Kit),
    picker: Picker,
    missing: Option<&Missing>,
    rows: f32,
) -> El {
    let keys = picker_keys(picker);
    let choice = s.picker(picker);
    let entries = s
        .book
        .entries()
        .iter()
        .map(|entry| {
            recipient_row(
                (k, choice_kit),
                picker,
                entry,
                choice.selected.contains(&entry.id),
            )
        })
        .collect();
    let other_issue = match missing {
        Some(Missing::OtherKey) => Some(MSG_BAD_KEY),
        Some(
            Missing::Input
            | Missing::Recipients
            | Missing::SavedKey { .. }
            | Missing::Identity
            | Missing::Files
            | Missing::ValidRows(_)
            | Missing::Folder,
        )
        | None => choice.other_key_bad.then_some(MSG_BAD_KEY),
    };
    let access: fn(&mut AppState) -> &mut TextEdit = match picker {
        Picker::Text => |s| &mut s.text.picker.other_key,
        Picker::Files => |s| &mut s.files.picker.other_key,
    };
    col()
        .w_full()
        .child(section_header(k, "Recipients").mb(SPACE_SM))
        .child(list_box(
            k,
            (keys.recipients, "Recipients"),
            entries,
            "No saved recipients yet. Add them in the Keys tab.",
            ListHeight::Rows(rows),
        ))
        .child(
            line_field(
                k,
                LineFieldSpec {
                    key: keys.other_key,
                    label: "Other key",
                    placeholder: "age1…",
                    issue: other_issue,
                },
                &choice.other_key,
                access,
                window_key,
                move |s: &mut AppState, tx: &mut Tx| {
                    let edited = match picker {
                        Picker::Text => Input::Text(TextInput::OtherKeyEdited),
                        Picker::Files => Input::Files(FilesInput::OtherKeyEdited),
                    };
                    dispatch(s, tx, edited);
                },
            )
            .mt(SPACE_MD),
        )
}

/// One recipient: its checkbox, which is the row's one node (named by the label, described by
/// the whole key), then the label and the shortened key. A press anywhere on the row toggles it.
fn recipient_row(
    (k, choice_kit): (&Kit, &Kit),
    picker: Picker,
    entry: &BookEntry,
    checked: bool,
) -> El {
    let id = entry.id;
    let toggle = move |s: &mut AppState, tx: &mut Tx| {
        dispatch(s, tx, toggled(picker, id));
    };
    let label = display_line(entry.label.chars());
    let checkbox = Checkbox::new(checked)
        .label(label.clone())
        .hide_label()
        .description(entry.public_key.clone())
        .on_toggle_tx(toggle)
        .build(choice_kit)
        .key(recipient_key(picker, id));
    row()
        .w_full()
        .min_h(CONTROL_HEIGHT)
        .gap(SPACE_SM)
        .center_y()
        .on_press_tx(toggle)
        .child(checkbox)
        .child(
            text(label)
                .font(TYPE_BODY)
                .color(k.c.text_base)
                .truncate()
                .min_w(0.0)
                .shrink()
                .a11y_hidden(),
        )
        .child(spacer())
        .child(
            text(truncated_key(&entry.public_key))
                .font(TYPE_MONO)
                .color(k.c.text_muted)
                .a11y_hidden(),
        )
}

/// The identity file both tabs unseal with, and "Choose identity file…".
pub fn identity(s: &AppState, k: &Kit, picker: Picker) -> El {
    let keys = picker_keys(picker);
    let shown = s.identity_path.as_ref().map_or_else(
        || "None chosen".to_owned(),
        |path| display_line(path.display().to_string().chars()),
    );
    let choose = Button::new("Choose identity file…")
        .variant(ButtonVariant::Ghost)
        .disabled(s.dialog.is_some())
        .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
            dispatch(s, tx, Input::DialogOpened(DialogPurpose::Identity(picker)));
        })
        .build(k)
        .key(keys.choose_identity);
    col()
        .w_full()
        .child(section_header(k, "Identity").mb(SPACE_SM))
        .child(
            row()
                .w_full()
                .gap(SPACE_MD)
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .key(keys.identity)
                        .child(detail_row(k, "Identity file", &shown)),
                )
                .child(choose),
        )
}
