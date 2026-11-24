//! The Text tab: Direction, the input, the recipients or the identity file, the output, and the
//! action row with Seal or Unseal.

use super::parts::{list_box, output_box};
use super::{standing_captions, window_key};
use crate::design::{FORM_MAX_WIDTH, TEXT_MIN_LINES, TEXT_RECIPIENT_ROWS};
use crate::services::dispatch;
use crate::state::notes::{Missing, Note};
use crate::state::recipients::{BookEntry, EntryId, MSG_BAD_KEY, truncated_key};
use crate::state::text::TextInput;
use crate::state::{AppState, Blocked, DialogPurpose, Input, Tab};
use common_gui_rune::keys::type_newline;
use common_gui_rune::parts::{
    EditorHeight, EditorSpec, LineFieldSpec, StatusLines, action_row, choice_switch, detail_row,
    field_label, line_field, reveal, section_header, status_line, text_editor,
};
use common_gui_rune::tokens::{CONTROL_HEIGHT, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XL, SPACE_XS};
use common_gui_rune::tokens::{TYPE_BODY, TYPE_MONO};
use common_utils::string_utils::display_line;
use rune::{Cx, El, KeyEvent, ScrollSpec, Tx, col, row, spacer, text};
use rune_kit::{Button, ButtonVariant, Checkbox, Kit, ScrollBar};
use shared_crypto::Direction;

/// The Text tab's scroll viewport.
pub const TEXT_SCROLL_KEY: &str = "seal-text-scroll";
/// What the scroll viewport holds: the form with its insets.
pub const TEXT_FORM_KEY: &str = "seal-text-form";
/// The Seal/Unseal switch.
pub const DIRECTION_KEY: &str = "seal-text-direction";
/// The input box.
pub const INPUT_KEY: &str = "seal-text-input";
/// The recipient list.
pub const RECIPIENTS_KEY: &str = "seal-text-recipients";
/// The Other key field.
pub const OTHER_KEY_KEY: &str = "seal-text-other-key";
/// The identity file row.
pub const IDENTITY_KEY: &str = "seal-text-identity";
/// "Choose identity file…".
pub const CHOOSE_IDENTITY_KEY: &str = "seal-text-choose-identity";
/// The output box.
pub const OUTPUT_KEY: &str = "seal-text-output";
/// "Copy output".
pub const COPY_OUTPUT_KEY: &str = "seal-text-copy-output";
/// The action row.
pub const TEXT_ACTIONS_KEY: &str = "seal-text-actions";
/// The action row's status line.
pub const TEXT_STATUS_KEY: &str = "seal-text-status";
/// Seal or Unseal.
pub const TEXT_SUBMIT_KEY: &str = "seal-text-submit";

const DIRECTIONS: [Direction; 2] = [Direction::Encrypt, Direction::Decrypt];
const DIRECTION_LABELS: [&str; 2] = ["Seal", "Unseal"];

/// The key of the checkbox for the recipient `id`.
pub fn recipient_key(id: EntryId) -> String {
    format!("seal-text-recipient-{}", id.number())
}

/// The Text tab.
pub fn view(s: &AppState, k: &Kit, cx: &mut Cx<AppState>) -> El {
    let viewport = cx.layout_with(TEXT_SCROLL_KEY, |rect| rect[3]);
    let missing = match s.note(Tab::Text) {
        Some(Note::Missing(missing)) => Some(missing),
        Some(_) | None => None,
    };
    let form = col()
        .w_full()
        .max_w(FORM_MAX_WIDTH)
        .grow(1.0)
        .child(direction(s, k))
        .child(input(s, k, missing).mt(SPACE_MD))
        .child(match s.text.direction {
            Direction::Encrypt => {
                reveal(recipients(s, k, missing)).key("seal-text-recipients-group")
            }
            Direction::Decrypt => reveal(identity(s, k)).key("seal-text-identity-group"),
        })
        .child(output(s, k).mt(SPACE_XL));
    let body = col()
        .key(TEXT_SCROLL_KEY)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            col()
                .key(TEXT_FORM_KEY)
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
    tab.child(actions(s, k))
}

/// Keys in the input box, before the box sees them: the window's shortcuts, and Return typing a
/// line break, which counts as an edit.
fn input_key(s: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if window_key(s, key, tx) {
        return true;
    }
    if type_newline(&mut s.text.input, key) {
        dispatch(s, tx, Input::Text(TextInput::InputEdited));
        return true;
    }
    false
}

fn direction(s: &AppState, k: &Kit) -> El {
    let selected = DIRECTIONS
        .iter()
        .position(|direction| *direction == s.text.direction)
        .unwrap_or(0);
    col()
        .w_full()
        .gap(SPACE_XS)
        .child(field_label(k, "Direction").a11y_hidden())
        .child(choice_switch(
            k,
            DIRECTION_KEY,
            "Direction",
            &DIRECTION_LABELS,
            selected,
            !s.running_on(Tab::Text),
            |s: &mut AppState, index: usize, tx: &mut Tx| {
                if let Some(direction) = DIRECTIONS.get(index) {
                    dispatch(s, tx, Input::Text(TextInput::DirectionPicked(*direction)));
                }
            },
        ))
}

fn input(s: &AppState, k: &Kit, missing: Option<&Missing>) -> El {
    let placeholder = match s.text.direction {
        Direction::Encrypt => "The text to seal",
        Direction::Decrypt => "Paste armored text (-----BEGIN AGE ENCRYPTED FILE-----)",
    };
    let spec = EditorSpec {
        key: INPUT_KEY,
        label: "Input",
        placeholder,
        description: None,
        invalid: missing == Some(&Missing::Input),
        height: EditorHeight::Fill {
            min_lines: TEXT_MIN_LINES,
        },
        font: TYPE_MONO,
    };
    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(SPACE_XS)
        .child(field_label(k, "Input").a11y_hidden())
        .child(text_editor(
            k,
            spec,
            &s.text.input,
            |s: &mut AppState| &mut s.text.input,
            input_key,
            |s: &mut AppState, tx: &mut Tx| {
                dispatch(s, tx, Input::Text(TextInput::InputEdited));
            },
        ))
}

fn recipients(s: &AppState, k: &Kit, missing: Option<&Missing>) -> El {
    let rows = s
        .book
        .entries()
        .iter()
        .map(|entry| recipient_row(k, entry, s.text.picker.selected.contains(&entry.id)))
        .collect();
    let other_issue = match missing {
        Some(Missing::OtherKey) => Some(MSG_BAD_KEY),
        Some(_) | None => s.text.picker.other_key_bad.then_some(MSG_BAD_KEY),
    };
    col()
        .w_full()
        .child(section_header(k, "Recipients").mb(SPACE_SM))
        .child(list_box(
            k,
            (RECIPIENTS_KEY, "Recipients"),
            rows,
            "No saved recipients yet. Add them in the Keys tab.",
            TEXT_RECIPIENT_ROWS,
        ))
        .child(
            line_field(
                k,
                LineFieldSpec {
                    key: OTHER_KEY_KEY,
                    label: "Other key",
                    placeholder: "age1…",
                    issue: other_issue,
                },
                &s.text.picker.other_key,
                |s: &mut AppState| &mut s.text.picker.other_key,
                window_key,
                |s: &mut AppState, tx: &mut Tx| {
                    dispatch(s, tx, Input::Text(TextInput::OtherKeyEdited));
                },
            )
            .mt(SPACE_MD),
        )
}

/// One recipient: its checkbox, which is the row's one node (named by the label, described by
/// the whole key), then the label and the shortened key. A press anywhere on the row toggles it.
fn recipient_row(k: &Kit, entry: &BookEntry, checked: bool) -> El {
    let id = entry.id;
    let toggle = move |s: &mut AppState, tx: &mut Tx| {
        dispatch(s, tx, Input::Text(TextInput::RecipientToggled(id)));
    };
    let label = display_line(entry.label.chars());
    let checkbox = Checkbox::new(checked)
        .label(label.clone())
        .hide_label()
        .description(entry.public_key.clone())
        .on_toggle_tx(toggle)
        .build(k)
        .key(recipient_key(id));
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

fn identity(s: &AppState, k: &Kit) -> El {
    let shown = s.identity_path.as_ref().map_or_else(
        || "None chosen".to_owned(),
        |path| display_line(path.display().to_string().chars()),
    );
    let choose = Button::new("Choose identity file…")
        .variant(ButtonVariant::Ghost)
        .disabled(s.dialog.is_some())
        .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
            dispatch(
                s,
                tx,
                Input::DialogOpened(DialogPurpose::Identity(Tab::Text)),
            );
        })
        .build(k)
        .key(CHOOSE_IDENTITY_KEY);
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
                        .key(IDENTITY_KEY)
                        .child(detail_row(k, "Identity file", &shown)),
                )
                .child(choose),
        )
}

fn output(s: &AppState, k: &Kit) -> El {
    let copy = Button::new("Copy output")
        .variant(ButtonVariant::Ghost)
        .disabled(s.text.output.is_empty() || s.running_on(Tab::Text))
        .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
            dispatch(s, tx, Input::Text(TextInput::CopyPressed));
        })
        .build(k)
        .key(COPY_OUTPUT_KEY);
    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(SPACE_XS)
        .child(
            row()
                .w_full()
                .h(CONTROL_HEIGHT)
                .center_y()
                .child(field_label(k, "Output").a11y_hidden())
                .child(spacer())
                .child(copy),
        )
        .child(output_box(
            k,
            OUTPUT_KEY,
            "Output",
            &s.text.output,
            TEXT_MIN_LINES,
        ))
}

fn actions(s: &AppState, k: &Kit) -> El {
    let label = match s.text.direction {
        Direction::Encrypt => "Seal",
        Direction::Decrypt => "Unseal",
    };
    let (variant, disabled) = match s.blocked() {
        None => (ButtonVariant::Submit, false),
        Some(Blocked::Running(job)) if job.tab() == Tab::Text => (ButtonVariant::Loading, false),
        Some(Blocked::Running(_) | Blocked::Stopped) => (ButtonVariant::Submit, true),
    };
    let submit = Button::new(label)
        .variant(variant)
        .disabled(disabled)
        .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
            dispatch(s, tx, Input::Text(TextInput::StartPressed));
        })
        .build(k)
        .key(TEXT_SUBMIT_KEY)
        .default_button();
    let status = s
        .status(Tab::Text)
        .map(|(tone, words)| status_line(k, TEXT_STATUS_KEY, tone, StatusLines::Wrap, &words));
    action_row(k, TEXT_ACTIONS_KEY, status, vec![submit])
}
