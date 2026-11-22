//! The time and text fields both sections' sidebars share: the Read section's capture rules and
//! the Export section's archive search. Each sidebar adds its own header, buttons, and notes.

use crate::design::parts;
use crate::design::tokens::{SIDEBAR_FIELD_WIDTH, SPACE_SM};
use crate::read::rules::{RulesForm, RulesIssues, TextMode, TimeMode, stamp};
use crate::state::{AppState, ListSection};
use rune::{El, TextEdit, col};
use rune_kit::{Button, ButtonVariant, Kit, Select, SelectAppearance, TextField};

type EditAccess = fn(&mut AppState) -> &mut TextEdit;

/// A text field of the form.
#[derive(Clone, Copy)]
enum Field {
    Start,
    End,
    Text,
}

/// `section`'s form.
pub fn form_of(s: &AppState, section: ListSection) -> &RulesForm {
    match section {
        ListSection::Read => &s.read.rules,
        ListSection::Export => &s.archive.form,
    }
}

fn form_of_mut(s: &mut AppState, section: ListSection) -> &mut RulesForm {
    match section {
        ListSection::Read => &mut s.read.rules,
        ListSection::Export => &mut s.archive.form,
    }
}

fn access(section: ListSection, field: Field) -> EditAccess {
    match (section, field) {
        (ListSection::Read, Field::Start) => |s| &mut s.read.rules.time_start,
        (ListSection::Read, Field::End) => |s| &mut s.read.rules.time_end,
        (ListSection::Read, Field::Text) => |s| &mut s.read.rules.text,
        (ListSection::Export, Field::Start) => |s| &mut s.archive.form.time_start,
        (ListSection::Export, Field::End) => |s| &mut s.archive.form.time_end,
        (ListSection::Export, Field::Text) => |s| &mut s.archive.form.text,
    }
}

/// Appends `section`'s time and text fields to `form`, with `issues` under the fields they
/// concern.
pub fn fields(
    s: &AppState,
    k: &Kit,
    layer: &mut Vec<El>,
    section: ListSection,
    issues: &RulesIssues,
    form: El,
) -> El {
    let rules = form_of(s, section);
    let form = form
        .child(parts::field_label(k, "Time"))
        .child(time_select(s, k, layer, section));
    let mut form = time_fields(rules, k, issues, section, form)
        .child(parts::field_label(k, "Text"))
        .child(text_select(s, k, layer, section));
    if rules.text_mode != TextMode::None {
        form = form.child(field(
            k,
            "Keywords or pattern",
            &rules.text,
            &issues.text,
            access(section, Field::Text),
        ));
    }
    form
}

fn time_select(s: &AppState, k: &Kit, layer: &mut Vec<El>, section: ListSection) -> El {
    let labels = TimeMode::ALL.map(TimeMode::label);
    let rules = form_of(s, section);
    Select::new("time-mode", &labels)
        .label("Time")
        .appearance(SelectAppearance::Field)
        .width(SIDEBAR_FIELD_WIDTH)
        .current(Some(rules.time_mode.label()))
        .open(rules.time_open)
        .on_toggle(move |s: &mut AppState| {
            let rules = form_of_mut(s, section);
            rules.time_open = !rules.time_open;
        })
        .on_close(move |s: &mut AppState| form_of_mut(s, section).time_open = false)
        .on_choose(move |s: &mut AppState, label: &str, _| {
            let rules = form_of_mut(s, section);
            if let Some(mode) = TimeMode::from_label(label) {
                rules.time_mode = mode;
            }
            rules.time_open = false;
        })
        .build(k, layer)
}

fn text_select(s: &AppState, k: &Kit, layer: &mut Vec<El>, section: ListSection) -> El {
    let labels = TextMode::ALL.map(TextMode::label);
    let rules = form_of(s, section);
    Select::new("text-mode", &labels)
        .label("Text")
        .appearance(SelectAppearance::Field)
        .width(SIDEBAR_FIELD_WIDTH)
        .current(Some(rules.text_mode.label()))
        .open(rules.text_open)
        .on_toggle(move |s: &mut AppState| {
            let rules = form_of_mut(s, section);
            rules.text_open = !rules.text_open;
        })
        .on_close(move |s: &mut AppState| form_of_mut(s, section).text_open = false)
        .on_choose(move |s: &mut AppState, label: &str, _| {
            let rules = form_of_mut(s, section);
            if let Some(mode) = TextMode::from_label(label) {
                rules.text_mode = mode;
            }
            rules.text_open = false;
        })
        .build(k, layer)
}

fn time_fields(
    rules: &RulesForm,
    k: &Kit,
    issues: &RulesIssues,
    section: ListSection,
    form: El,
) -> El {
    let start_label = match rules.time_mode {
        TimeMode::None => return form,
        TimeMode::Between => "Start",
        TimeMode::OlderThan
        | TimeMode::OlderOrEqual
        | TimeMode::NewerThan
        | TimeMode::NewerOrEqual
        | TimeMode::During => "Time (UTC)",
    };
    let form = form.child(time_field(
        k,
        start_label,
        &rules.time_start,
        &issues.time_start,
        access(section, Field::Start),
    ));
    match rules.time_mode {
        TimeMode::Between => form.child(time_field(
            k,
            "End",
            &rules.time_end,
            &issues.time_end,
            access(section, Field::End),
        )),
        TimeMode::None
        | TimeMode::OlderThan
        | TimeMode::OlderOrEqual
        | TimeMode::NewerThan
        | TimeMode::NewerOrEqual
        | TimeMode::During => form,
    }
}

/// A timestamp field with a Now button that writes the current time into it.
fn time_field(
    k: &Kit,
    label: &str,
    edit: &TextEdit,
    issue: &Option<String>,
    access: EditAccess,
) -> El {
    let now = Button::new("Now")
        .variant(ButtonVariant::Ghost)
        .on_press(move |s: &mut AppState| {
            access(s).replace_text(stamp(&chrono::Utc::now()));
        })
        .build(k);
    field(k, label, edit, issue, access).child(now)
}

fn field(k: &Kit, label: &str, edit: &TextEdit, issue: &Option<String>, access: EditAccess) -> El {
    let mut field = TextField::new(label, edit).on_edit(access);
    if let Some(issue) = issue {
        field = field.error(issue.clone());
    }
    col().gap(SPACE_SM).w_full().child(field.build(k))
}
