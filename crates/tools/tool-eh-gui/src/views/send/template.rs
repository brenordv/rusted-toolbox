//! The Template switch's parts of the Send section: the switch and its placeholder list in the
//! body's caption row, and the sample block under the editor.

use crate::design::parts::{self, LabelSide};
use crate::design::tokens::{SPACE_MD, SPACE_SM, SPACE_XS, TYPE_CAPTION, TYPE_MONO};
use crate::read::rows::{display_line, sentence};
use crate::send::template::{TemplateState, shown_sample};
use crate::state::AppState;
use common_mock_data::template::{OPTIONS_HELP, placeholder_groups};
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit, Placement, Popover, ScrollBar, popover};

/// The Template switch's key.
pub const SWITCH_KEY: &str = "send-template-switch";
/// The "Show placeholders" button's key.
pub const PLACEHOLDERS_KEY: &str = "send-placeholders";
/// The placeholder list's scroll viewport key.
pub const PLACEHOLDER_LIST_KEY: &str = "send-placeholder-list";
/// The sample block's key.
pub const SAMPLE_KEY: &str = "send-sample";
/// The "Regenerate" button's key.
pub const REGENERATE_KEY: &str = "send-regenerate";

const NO_PLACEHOLDERS: &str = "The template has no placeholders; every event is the same";

/// The switch with its label and, while on, the button that lists the placeholders.
pub fn switch_controls(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> El {
    let composer = &s.send.composer;
    let on = composer.template().is_on();
    let mut controls = row().gap(SPACE_SM).center_y().child(parts::labeled_switch(
        k,
        SWITCH_KEY,
        "Template",
        LabelSide::Trailing,
        on,
        |s: &mut AppState| {
            let composer = &mut s.send.composer;
            let on = !composer.template().is_on();
            composer.set_template(on);
        },
    ));
    if on {
        controls = controls.child(placeholders(s, k, layer));
    }
    controls
}

fn placeholders(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> El {
    let trigger = Button::new("Show placeholders")
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Large)
        .on_press(|s: &mut AppState| {
            let composer = &mut s.send.composer;
            composer.placeholders_open = !composer.placeholders_open;
        })
        .build(k)
        .key(PLACEHOLDERS_KEY);
    let mut list = col().gap(SPACE_MD).w_full();
    for (prefix, commands) in placeholder_groups() {
        list = list.child(
            col()
                .gap(SPACE_XS)
                .w_full()
                .child(text(prefix).font(TYPE_CAPTION).color(k.c.text_muted))
                .child(mono_lines(k, &commands.join("\n"))),
        );
    }
    list = list.child(
        col()
            .gap(SPACE_XS)
            .w_full()
            .child(text("Options").font(TYPE_CAPTION).color(k.c.text_muted))
            .child(mono_lines(k, OPTIONS_HELP)),
    );
    Popover::new("placeholders-popover")
        .trigger(trigger)
        .title("Placeholders")
        .child(
            // The kit popover's body shrinks to the window; the list viewport must be its direct
            // child to shrink with it.
            popover::fit_scroll(list)
                .key(PLACEHOLDER_LIST_KEY)
                .scroll_bar(k),
        )
        .open(s.send.composer.placeholders_open)
        .placement(Placement::BottomEnd)
        .on_close(|s: &mut AppState| s.send.composer.placeholders_open = false)
        .build(k, layer)
}

fn mono_lines(k: &Kit, lines: &str) -> El {
    text(lines)
        .font(TYPE_MONO)
        .color(k.c.text_base)
        .wrap()
        .selectable(true)
}

/// The sample block while the switch is on: the latest render, or why there is none.
pub fn sample(s: &AppState, k: &Kit) -> Option<El> {
    let state = s.send.composer.template();
    let mut notes = Vec::new();
    let body = match state {
        TemplateState::Off => return None,
        TemplateState::OverCap => parts::caption(k, "No sample for bodies over 1 MB"),
        TemplateState::Invalid(problem) => parts::caption(k, &display_line(problem.chars())),
        TemplateState::Ready {
            template, sample, ..
        } => {
            if template.placeholder_count() == 0 {
                notes.push(parts::caption(k, NO_PLACEHOLDERS));
            }
            for note in template.inert_locale_notes() {
                notes.push(parts::caption(k, &display_line(sentence(note).chars())));
            }
            match sample {
                Ok(sample) => {
                    let (shown, cut) = shown_sample(sample);
                    let mut body = col()
                        .gap(SPACE_SM)
                        .w_full()
                        .child(parts::body_text(k, shown));
                    if cut {
                        body = body.child(parts::caption(k, "Sample cut at 16 KB"));
                    }
                    body
                }
                Err(error) => parts::caption(
                    k,
                    &format!("This sample failed: {}", display_line(error.chars())),
                ),
            }
        }
    };
    let regenerate = Button::new("Regenerate")
        .variant(ButtonVariant::Ghost)
        .disabled(state.template().is_none())
        .on_press(|s: &mut AppState| {
            s.send.composer.regenerate();
        })
        .build(k)
        .key(REGENERATE_KEY);
    let block = parts::sample_block(k, SAMPLE_KEY, "Sample", regenerate, body);
    Some(
        col()
            .key("send-sample-area")
            .gap(SPACE_SM)
            .w_full()
            .child(block)
            .children(notes),
    )
}
