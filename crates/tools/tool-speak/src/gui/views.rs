//! The speak window: the area switch over Compose (the text, the reaction buttons, Generate,
//! Play) or Settings (the form and Save).

use super::services::dispatch;
use super::state::{Area, Field, Input, Method, SpeakState, reaction_word};
use crate::config::SPEECH_PATH;
use crate::design::{
    FORM_MAX_WIDTH, REACTION_LABEL_MAX, REACTION_LABEL_MAX_CHARS, REACTION_LINES,
    REACTION_ROWS_MAX, TEXT_MIN_LINES,
};
use common_gui_rune::keys::{area_chord, is_chord, type_newline};
use common_gui_rune::motion::CONTROL_PAIR;
use common_gui_rune::parts::{
    EditorHeight, EditorSpec, LineFieldSpec, StatusLines, Tone, action_row, caption, choice_switch,
    detail_row, field_label, hairline, line_field, status_line, text_editor,
};
use common_gui_rune::tokens::{
    CONTROL_HEIGHT, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XS, TYPE_BODY, TYPE_MONO,
};
use common_gui_rune::{Surface, surface};
use common_utils::string_utils::display_line;
use rune::{Cx, El, KeyEvent, ScrollSpec, TextEdit, Transition, Tx, col, row};
use rune_kit::{Button, ButtonVariant, Kit, ScrollBar, Select, SelectAppearance};
use tracing::info;

/// The window's title in the OS.
pub const OS_TITLE: &str = "speak";

/// The area switch.
pub const AREAS_KEY: &str = "speak-areas";
/// The Compose area.
pub const COMPOSE_KEY: &str = "speak-compose";
/// The Settings area.
pub const SETTINGS_KEY: &str = "speak-settings";
/// The Compose text editor.
pub const TEXT_KEY: &str = "speak-text";
/// The group of reaction buttons.
pub const REACTIONS_KEY: &str = "speak-reactions";
/// The reaction buttons' scroll viewport.
pub const REACTIONS_SCROLL_KEY: &str = "speak-reactions-scroll";
/// Compose's fixed row: the status, Play, and Generate.
pub const COMPOSE_ACTIONS_KEY: &str = "speak-compose-actions";
/// Compose's status line.
pub const COMPOSE_STATUS_KEY: &str = "speak-compose-status";
/// The Play button.
pub const PLAY_KEY: &str = "speak-play";
/// The Generate button.
pub const GENERATE_KEY: &str = "speak-generate";
/// The Settings form's scroll viewport.
pub const SETTINGS_SCROLL_KEY: &str = "speak-settings-scroll";
/// The Server field.
pub const SERVER_KEY: &str = "speak-server";
/// The Endpoint field.
pub const ENDPOINT_KEY: &str = "speak-endpoint";
/// The request method picker, as the kit keys its parts.
const METHOD_PICKER: &str = "speak-method";
/// The request method picker's field: the kit keys it after the picker.
pub const METHOD_KEY: &str = "speak-method-trigger";
/// The Model field.
pub const MODEL_KEY: &str = "speak-model";
/// The Voice field.
pub const VOICE_KEY: &str = "speak-voice";
/// The reactions box.
pub const REACTIONS_BOX_KEY: &str = "speak-reactions-box";
/// The reactions box's issue line.
pub const REACTIONS_ISSUE_KEY: &str = "speak-reactions-box-issue";
/// Settings' fixed row: the save status and Save.
pub const SETTINGS_FOOTER_KEY: &str = "speak-settings-footer";
/// The save status line.
pub const SAVE_STATUS_KEY: &str = "speak-save-status";
/// The Save button.
pub const SAVE_KEY: &str = "speak-save";

/// The Server field's placeholder: the shape of a server's address.
pub const SERVER_PLACEHOLDER: &str = "http://host:10157";
/// The hint under the reactions box.
pub const REACTIONS_HINT: &str = "One per line, in square brackets";

/// The key of the reaction button at `index`.
pub fn reaction_key(index: usize) -> String {
    format!("speak-reaction-{index}")
}

/// The key of `field`'s control, where focus goes when the field has an issue.
pub fn field_key(field: Field) -> &'static str {
    match field {
        Field::Server => SERVER_KEY,
        Field::Endpoint => ENDPOINT_KEY,
        Field::Method => METHOD_KEY,
        Field::Model => MODEL_KEY,
        Field::Voice => VOICE_KEY,
        Field::Reactions => REACTIONS_BOX_KEY,
    }
}

/// Runs when the window is first on screen: a first run puts focus in the Server field. True
/// when it moved focus.
pub fn shown(s: &mut SpeakState, tx: &mut Tx) -> bool {
    let first_run = s.config().is_none();
    if first_run {
        tx.focus(SERVER_KEY, true);
    }
    info!(area = s.area().as_str(), first_run, "speak: window shown");
    first_run
}

/// Sets the appearance the window draws in (true: dark); true when it changed.
pub fn set_appearance(s: &mut SpeakState, dark: bool) -> bool {
    s.theme.set(dark)
}

/// The window's shortcuts: `mod+1` and `mod+2` pick an area as a fresh press, and `mod+enter`
/// generates while Compose shows. True when `key` was one of them. A focused text field takes
/// these chords for itself, so each field runs this before its own keys.
pub fn window_key(s: &mut SpeakState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if let Some(area) = area_chord(Area::ALL.len(), key, tx).and_then(|index| Area::ALL.get(index))
    {
        dispatch(s, Input::AreaPicked(*area));
        return true;
    }
    if is_chord("mod+enter", key, tx) {
        return match s.area() {
            Area::Compose => {
                dispatch(s, Input::GeneratePressed);
                true
            }
            Area::Settings => false,
        };
    }
    false
}

/// Keys in the Compose text, before the editor sees them: the window's shortcuts, and Return
/// typing a line break.
fn text_key(s: &mut SpeakState, key: &KeyEvent, tx: &mut Tx) -> bool {
    window_key(s, key, tx) || type_newline(&mut s.text, key)
}

/// Keys in the reactions box, before the editor sees them: the window's shortcuts, and Return
/// typing a line break, which counts as an edit of the field (the editor's change hook does
/// not run for it).
fn reactions_key(s: &mut SpeakState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if window_key(s, key, tx) {
        return true;
    }
    if type_newline(&mut s.form.reactions, key) {
        dispatch(s, Input::FieldEdited(Field::Reactions));
        return true;
    }
    false
}

/// The window's root.
pub fn view(s: &SpeakState, cx: &mut Cx<SpeakState>) -> El {
    let k = s.theme.kit(cx.system_dark());
    let mut layer = Vec::new();
    let areas = choice_switch(
        k,
        AREAS_KEY,
        "Area",
        &Area::LABELS,
        s.area().index(),
        true,
        |s: &mut SpeakState, index: usize, _: &mut Tx| {
            if let Some(area) = Area::ALL.get(index) {
                dispatch(s, Input::AreaPicked(*area));
            }
        },
    );
    let area = match s.area() {
        Area::Compose => compose(s, k).key(COMPOSE_KEY),
        Area::Settings => settings(s, k, &mut layer).key(SETTINGS_KEY),
    };
    let window = col()
        .w_full()
        .h_full()
        .fill(surface(k, Surface::Window))
        .child(row().w_full().padding(SPACE_LG).child(areas))
        .child(hairline(k))
        .child(
            col()
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .fill(surface(k, Surface::Content))
                .child(area.transition(Transition::opacity().springs(CONTROL_PAIR))),
        );
    rune_kit::root(window, layer)
}

/// A column that takes the height it is given and can shrink below its content's.
fn filling_col() -> El {
    col().w_full().grow(1.0).min_h(0.0)
}

/// Compose: the text, the reaction buttons, and a fixed row with the status, Play, and
/// Generate.
fn compose(s: &SpeakState, k: &Kit) -> El {
    let spec = EditorSpec {
        key: TEXT_KEY,
        label: "Text",
        placeholder: "What speak should say",
        description: None,
        invalid: false,
        height: EditorHeight::Fill {
            min_lines: TEXT_MIN_LINES,
        },
        font: TYPE_BODY,
    };
    let editor = text_editor(
        k,
        spec,
        &s.text,
        |s: &mut SpeakState| &mut s.text,
        text_key,
        |_: &mut SpeakState, _: &mut Tx| {},
    );
    let mut body = filling_col()
        .gap(SPACE_SM)
        .padding(SPACE_LG)
        .child(field_label(k, "Text").a11y_hidden())
        .child(editor);
    if let Some(reactions) = reactions_group(s, k) {
        body = body.child(reactions);
    }
    filling_col().child(body).child(compose_actions(s, k))
}

/// The reaction buttons in a wrapping row, under their label; `None` when the saved settings
/// list no reaction. Past [`REACTION_ROWS_MAX`] rows the block scrolls.
fn reactions_group(s: &SpeakState, k: &Kit) -> Option<El> {
    let reactions = s.reactions();
    if reactions.is_empty() {
        return None;
    }
    let buttons = reactions
        .iter()
        .enumerate()
        .map(|(index, tag)| reaction_button(k, index, tag))
        .collect::<Vec<_>>();
    let rows_height = REACTION_ROWS_MAX * CONTROL_HEIGHT + (REACTION_ROWS_MAX - 1.0) * SPACE_SM;
    let viewport = col()
        .key(REACTIONS_SCROLL_KEY)
        .w_full()
        .max_h(rows_height)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(row().w_full().flex_wrap().gap(SPACE_SM).children(buttons))
        .scroll_bar(k);
    Some(
        col()
            .key(REACTIONS_KEY)
            .role("group")
            .label("Vocal reactions")
            .w_full()
            .gap(SPACE_XS)
            .child(field_label(k, "Vocal reactions").a11y_hidden())
            .child(viewport),
    )
}

/// A button that types `tag` into the text. It shows the tag on one line, cut past
/// [`REACTION_LABEL_MAX_CHARS`] with the whole tag as its help text, and is named "Insert" and
/// the tag's words.
fn reaction_button(k: &Kit, index: usize, tag: &str) -> El {
    let tag = display_line(tag.chars());
    let cut = tag.chars().count() > REACTION_LABEL_MAX_CHARS;
    let shown = if cut {
        let start: String = tag.chars().take(REACTION_LABEL_MAX_CHARS).collect();
        format!("{}…", start.trim_end())
    } else {
        tag.clone()
    };
    let button = Button::new(shown)
        .variant(ButtonVariant::Outline)
        .on_press_tx(move |s: &mut SpeakState, tx: &mut Tx| press_reaction(s, index, tx))
        .build(k)
        .key(reaction_key(index))
        .label(format!("Insert {}", reaction_word(&tag)))
        .max_w(REACTION_LABEL_MAX);
    if cut { button.help(tag) } else { button }
}

/// Types the reaction at `index` into the text at the caret and puts focus back in the
/// editor.
fn press_reaction(s: &mut SpeakState, index: usize, tx: &mut Tx) {
    let Some(tag) = s.reactions().get(index).cloned() else {
        return;
    };
    s.insert_reaction(&tag);
    tx.focus(TEXT_KEY, false);
}

/// A wrapping status line keyed `key` for an action row, when there is something to say.
fn row_status(k: &Kit, key: &str, words: Option<(Tone, String)>) -> Option<El> {
    words.map(|(tone, words)| status_line(k, key, tone, StatusLines::Wrap, &words))
}

fn compose_actions(s: &SpeakState, k: &Kit) -> El {
    let play = Button::new("Play")
        .variant(ButtonVariant::Neutral)
        .disabled(!s.can_play())
        .on_press(|s: &mut SpeakState| {
            dispatch(s, Input::PlayPressed);
        })
        .build(k)
        .key(PLAY_KEY);
    let generate = Button::new("Generate")
        .variant(if s.is_generating() {
            ButtonVariant::Loading
        } else {
            ButtonVariant::Submit
        })
        .disabled(!s.can_generate())
        .on_press(|s: &mut SpeakState| {
            dispatch(s, Input::GeneratePressed);
        })
        .build(k)
        .key(GENERATE_KEY);
    action_row(
        k,
        COMPOSE_ACTIONS_KEY,
        row_status(k, COMPOSE_STATUS_KEY, s.compose_status()),
        vec![play, generate],
    )
}

/// Settings: the form in a scroll body, and a fixed row with the save status and Save.
fn settings(s: &SpeakState, k: &Kit, layer: &mut Vec<El>) -> El {
    let issues = s.issues();
    let form = col()
        .w_full()
        .max_w(FORM_MAX_WIDTH)
        .gap(SPACE_MD)
        .child(settings_field(
            k,
            (Field::Server, "Server", SERVER_PLACEHOLDER),
            &s.form.server,
            issues.server.as_deref(),
            |s| &mut s.form.server,
        ))
        .child(settings_field(
            k,
            (Field::Endpoint, "Endpoint", SPEECH_PATH),
            &s.form.endpoint,
            None,
            |s| &mut s.form.endpoint,
        ))
        .child(method_picker(s, k, layer))
        .child(settings_field(
            k,
            (Field::Model, "Model", ""),
            &s.form.model,
            issues.model.as_deref(),
            |s| &mut s.form.model,
        ))
        .child(settings_field(
            k,
            (Field::Voice, "Voice", ""),
            &s.form.voice,
            issues.voice.as_deref(),
            |s| &mut s.form.voice,
        ))
        .child(reactions_box(s, k))
        .child(detail_row(
            k,
            "Saves to",
            &display_line(s.config_path().display().to_string().chars()),
        ));
    let body = filling_col()
        .key(SETTINGS_SCROLL_KEY)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(col().w_full().padding(SPACE_LG).child(form))
        .scroll_bar(k);
    let save = Button::new("Save")
        .variant(if s.is_saving() {
            ButtonVariant::Loading
        } else {
            ButtonVariant::Submit
        })
        .on_press_tx(press_save)
        .build(k)
        .key(SAVE_KEY);
    filling_col().child(body).child(action_row(
        k,
        SETTINGS_FOOTER_KEY,
        row_status(k, SAVE_STATUS_KEY, s.save_words()),
        vec![save],
    ))
}

/// Save's press: saves a form that validates; otherwise shows each field's issue and moves
/// focus to the first field that has one.
fn press_save(s: &mut SpeakState, tx: &mut Tx) {
    dispatch(s, Input::SavePressed);
    if let Some(field) = s.issues().first() {
        tx.focus(field_key(field), true);
    }
}

/// One single-line Settings field: `(field, label, placeholder)`, its text, and its issue.
fn settings_field(
    k: &Kit,
    (field, label, placeholder): (Field, &str, &str),
    edit: &TextEdit,
    issue: Option<&str>,
    access: fn(&mut SpeakState) -> &mut TextEdit,
) -> El {
    let spec = LineFieldSpec {
        key: field_key(field),
        label,
        placeholder,
        issue,
    };
    line_field(
        k,
        spec,
        edit,
        access,
        window_key,
        move |s: &mut SpeakState, _: &mut Tx| {
            dispatch(s, Input::FieldEdited(field));
        },
    )
}

/// The request method picker under its label.
fn method_picker(s: &SpeakState, k: &Kit, layer: &mut Vec<El>) -> El {
    let picker = Select::new(METHOD_PICKER, &Method::LABELS)
        .label("Request method")
        .appearance(SelectAppearance::Field)
        .current(Some(s.form.method.label()))
        .open(s.form.method_open)
        .on_toggle(|s: &mut SpeakState| s.form.method_open = !s.form.method_open)
        .on_close(|s: &mut SpeakState| s.form.method_open = false)
        .on_choose(|s: &mut SpeakState, label: &str, _| {
            s.form.method_open = false;
            let Some(method) = Method::from_label(label) else {
                return;
            };
            if s.form.method != method {
                s.form.method = method;
                dispatch(s, Input::FieldEdited(Field::Method));
            }
        })
        .build(k, layer);
    col()
        .w_full()
        .gap(SPACE_XS)
        .child(field_label(k, "Request method").a11y_hidden())
        .child(picker)
}

/// The reaction tags, one per line, with the hint under the box and the issue under the hint.
fn reactions_box(s: &SpeakState, k: &Kit) -> El {
    let issue = s.issues().reactions.as_deref();
    let spec = EditorSpec {
        key: REACTIONS_BOX_KEY,
        label: "Vocal reactions",
        placeholder: "[laugh]",
        description: Some(REACTIONS_HINT),
        invalid: issue.is_some(),
        height: EditorHeight::Fixed {
            lines: REACTION_LINES,
        },
        font: TYPE_MONO,
    };
    let mut field = col()
        .w_full()
        .gap(SPACE_XS)
        .child(field_label(k, "Vocal reactions").a11y_hidden())
        .child(text_editor(
            k,
            spec,
            &s.form.reactions,
            |s: &mut SpeakState| &mut s.form.reactions,
            reactions_key,
            |s: &mut SpeakState, _: &mut Tx| {
                dispatch(s, Input::FieldEdited(Field::Reactions));
            },
        ))
        .child(caption(k, REACTIONS_HINT).a11y_hidden());
    if let Some(issue) = issue {
        field = field.child(status_line(
            k,
            REACTIONS_ISSUE_KEY,
            Tone::Danger,
            StatusLines::Wrap,
            issue,
        ));
    }
    field
}

#[cfg(test)]
mod tests;
