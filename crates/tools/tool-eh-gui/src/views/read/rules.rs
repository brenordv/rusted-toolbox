//! The Read sidebar: capture rules that decide which captured messages the list shows and the
//! message files hold.

use crate::app::dispatch;
use crate::design::parts;
use crate::design::tokens::{SPACE_LG, SPACE_MD, SPACE_SM};
use crate::read::rules::rule_count;
use crate::read::{ReadInput, rules_editable};
use crate::state::{AppState, ConnectionState, Input, Section};
use crate::views::rules_form;
use rune::{El, col, row};
use rune_kit::{Badge, BadgeVariant, Button, ButtonVariant, Kit};

pub fn sidebar(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> El {
    let parsed = s.read.rules.parsed();
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
    let form = col()
        .key("capture-rules")
        .gap(SPACE_MD)
        .px(SPACE_LG)
        .child(header)
        .child(parts::caption(
            k,
            "Rules apply to messages captured from now on. Rows already in the list stay.",
        ));
    let mut form = rules_form::fields(s, k, layer, Section::Read, &issues, form).child(
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
