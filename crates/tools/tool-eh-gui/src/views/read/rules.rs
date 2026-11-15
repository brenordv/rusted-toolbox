//! The Read sidebar: capture rules that decide which captured messages the list shows and the
//! message files hold.

use crate::app::dispatch;
use crate::design::parts;
use crate::design::tokens::{SIDEBAR_FIELD_WIDTH, SPACE_LG, SPACE_MD, SPACE_SM};
use crate::read::rules::{RulesIssues, TextMode, TimeMode, rule_count, stamp};
use crate::read::{ReadInput, rules_editable};
use crate::state::{AppState, ConnectionState, Input};
use rune::{El, TextEdit, col, row};
use rune_kit::{
    Badge, BadgeVariant, Button, ButtonVariant, Kit, Select, SelectAppearance, TextField,
};

type EditAccess = fn(&mut AppState) -> &mut TextEdit;

pub fn sidebar(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> El {
    let rules = &s.read.rules;
    let parsed = rules.parsed();
    let issues = parsed.as_ref().err().cloned().unwrap_or_default();
    let editable = rules_editable(&s.connection);
    let applicable = editable
        && parsed
            .as_ref()
            .is_ok_and(|draft| *draft != s.read.rules_applied);
    let clearable = editable && rule_count(&s.read.rules_applied) > 0;

    let header = row()
        .gap(SPACE_SM)
        .center_y()
        .child(parts::section_header(k, "Capture rules"))
        .child(
            Badge::new(rule_count(&s.read.rules_applied).to_string())
                .variant(BadgeVariant::Subtle)
                .build(k)
                .mt(SPACE_LG),
        );
    let mut form = col()
        .key("capture-rules")
        .gap(SPACE_MD)
        .px(SPACE_LG)
        .child(header)
        .child(parts::caption(
            k,
            "Rules apply to messages captured from now on. Rows already in the list stay.",
        ))
        .child(parts::field_label(k, "Time"))
        .child(time_select(s, k, layer));
    form = time_fields(s, k, &issues, form)
        .child(parts::field_label(k, "Text"))
        .child(text_select(s, k, layer));
    if rules.text_mode != TextMode::None {
        form = form.child(field(
            k,
            "Keywords or pattern",
            &rules.text,
            &issues.text,
            |s| &mut s.read.rules.text,
        ));
    }
    form = form.child(
        row()
            .gap(SPACE_SM)
            .child(
                Button::new("Apply rules")
                    .variant(ButtonVariant::Submit)
                    .disabled(!applicable)
                    .on_press(|s: &mut AppState| dispatch(s, Input::Read(ReadInput::ApplyRules)))
                    .build(k)
                    .key("apply-rules"),
            )
            .child(
                Button::new("Clear")
                    .variant(ButtonVariant::Ghost)
                    .disabled(!clearable)
                    .on_press(|s: &mut AppState| dispatch(s, Input::Read(ReadInput::ClearRules)))
                    .build(k)
                    .key("clear-rules"),
            ),
    );
    if let Some(note) = connection_note(&s.connection) {
        form = form.child(parts::caption(k, note));
    }
    form
}

/// Where applied rules go, when that is not the running session.
fn connection_note(connection: &ConnectionState) -> Option<&'static str> {
    match connection {
        ConnectionState::Connected { .. }
        | ConnectionState::Connecting {
            session: Some(_), ..
        } => None,
        ConnectionState::Connecting { session: None, .. } => {
            Some("Rules can change once connected.")
        }
        ConnectionState::Disconnected
        | ConnectionState::Stopping { .. }
        | ConnectionState::Failed(_) => Some("Rules apply when you connect."),
    }
}

fn time_select(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> El {
    let labels = TimeMode::ALL.map(TimeMode::label);
    Select::new("time-mode", &labels)
        .label("Time")
        .appearance(SelectAppearance::Field)
        .width(SIDEBAR_FIELD_WIDTH)
        .current(Some(s.read.rules.time_mode.label()))
        .open(s.read.rules.time_open)
        .on_toggle(|s: &mut AppState| s.read.rules.time_open = !s.read.rules.time_open)
        .on_close(|s: &mut AppState| s.read.rules.time_open = false)
        .on_choose(|s: &mut AppState, label: &str, _| {
            if let Some(mode) = TimeMode::from_label(label) {
                s.read.rules.time_mode = mode;
            }
            s.read.rules.time_open = false;
        })
        .build(k, layer)
}

fn text_select(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> El {
    let labels = TextMode::ALL.map(TextMode::label);
    Select::new("text-mode", &labels)
        .label("Text")
        .appearance(SelectAppearance::Field)
        .width(SIDEBAR_FIELD_WIDTH)
        .current(Some(s.read.rules.text_mode.label()))
        .open(s.read.rules.text_open)
        .on_toggle(|s: &mut AppState| s.read.rules.text_open = !s.read.rules.text_open)
        .on_close(|s: &mut AppState| s.read.rules.text_open = false)
        .on_choose(|s: &mut AppState, label: &str, _| {
            if let Some(mode) = TextMode::from_label(label) {
                s.read.rules.text_mode = mode;
            }
            s.read.rules.text_open = false;
        })
        .build(k, layer)
}

fn time_fields(s: &AppState, k: &Kit, issues: &RulesIssues, form: El) -> El {
    let rules = &s.read.rules;
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
        |s| &mut s.read.rules.time_start,
    ));
    match rules.time_mode {
        TimeMode::Between => form.child(time_field(
            k,
            "End",
            &rules.time_end,
            &issues.time_end,
            |s| &mut s.read.rules.time_end,
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
