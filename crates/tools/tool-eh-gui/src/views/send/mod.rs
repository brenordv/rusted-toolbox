//! The Send section: the body editor over the action row that shows the run.

pub mod sidebar;

use crate::app::dispatch;
use crate::design::tokens::{
    ICON_SIZE, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XS, TYPE_BODY, TYPE_BODY_NUM, TYPE_CAPTION,
    TYPE_LABEL,
};
use crate::design::{motion, parts};
use crate::read::rows::display_line;
use crate::send::SendInput;
use crate::send::compose::{BODY_CAP, SendField};
use crate::send::run::SendRun;
use crate::state::{AppState, Input, Section};
use crate::views::read::short_duration;
use common_utils::string_utils::{format_bytes_to_string, format_thousands};
use rune::{
    Chord, El, Key, KeyEvent, LiveRegion, Mods, SpringPair, Transition, Tx, col, icon, row, spacer,
    text,
};
use rune_kit::{Button, ButtonVariant, Kit, RollingText, Spinner};
use shared_eventhub_core::producer::SendEnd;

/// The body editor's key.
pub const BODY_KEY: &str = "send-body";
/// The Send button's key.
pub const SEND_BUTTON_KEY: &str = "send-button";
/// The status block's key prefix; the phase word follows it.
pub const STATUS_KEY: &str = "send-status";

/// The element focus moves to for `field`.
pub fn field_key(field: SendField) -> &'static str {
    match field {
        SendField::Target => sidebar::EDIT_TARGET_KEY,
        SendField::PartitionKey => sidebar::PARTITION_KEY_KEY,
        SendField::Count => sidebar::COUNT_KEY,
        SendField::Interval => sidebar::INTERVAL_KEY,
        SendField::Body => BODY_KEY,
    }
}

/// Presses Send and moves focus to the first part with an issue, if the press found any.
pub fn press_send(s: &mut AppState, tx: &mut Tx) {
    dispatch(s, Input::Send(SendInput::SendPressed));
    if let Some(field) = s.send.take_focus() {
        tx.focus(field_key(field), true);
    }
}

/// The shortcuts that work while typing in the Send section and with nothing focused, none of
/// them while a sheet is open: `mod+enter` sends while the Send section shows, and `mod+1..3`
/// pick a section. True when `key` was one of them and acted.
pub fn send_keys(s: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if s.sheet.is_some() {
        return false;
    }
    let chord = |text: &str| Chord::parse(text).is_ok_and(|chord| chord.matches_tx(key, tx));
    if chord("mod+enter") {
        if s.section != Section::Send {
            return false;
        }
        press_send(s, tx);
        return true;
    }
    for (index, section) in Section::ALL.into_iter().enumerate() {
        if chord(&format!("mod+{}", index + 1)) {
            dispatch(s, Input::SectionPicked(section));
            return true;
        }
    }
    false
}

/// The body editor's keys: the Send shortcuts, and Return (with or without Shift) typing a
/// newline. A field without `on_submit` hands a plain Return to the window's default button,
/// and the Send section has none, so the body types the newline itself.
fn body_keys(s: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if send_keys(s, key, tx) {
        return true;
    }
    let newline = key.key == Key::Enter
        && Mods {
            shift: key.mods.shift,
            ..Mods::default()
        } == key.mods;
    if newline {
        s.send.composer.body.insert("\n");
    }
    newline
}

pub fn content(s: &AppState, k: &Kit) -> El {
    let send = &s.send;
    let issues = send.shown_issues();
    let bytes = send.composer.body.text.len();
    let size = text(format_bytes_to_string(bytes as u64))
        .font(TYPE_CAPTION)
        .color(if bytes > BODY_CAP {
            k.c.danger.fg
        } else {
            k.c.text_muted
        });
    let caption_row = row()
        .key("send-body-caption")
        .w_full()
        .center_y()
        .px(SPACE_LG)
        .pt(SPACE_LG)
        .pb(SPACE_SM)
        .child(text("Event body").font(TYPE_LABEL).color(k.c.text_base))
        .child(spacer())
        .child(size);
    let editor = parts::body_editor(
        k,
        BODY_KEY,
        &send.composer.body,
        issues.body.is_some(),
        |s: &mut AppState| &mut s.send.composer.body,
        body_keys,
    );
    let mut body = col()
        .key("send-body-area")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(SPACE_SM)
        .px(SPACE_LG)
        .pb(SPACE_LG)
        .child(editor);
    if let Some(issue) = &issues.body {
        body = body.child(parts::issue(k, issue));
    }
    col()
        .key("send-content")
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(caption_row)
        .child(body)
        .child(parts::hairline(k))
        .child(action_row(s, k))
}

/// The fixed row under the body: the run's status, Cancel while it runs, and Send.
fn action_row(s: &AppState, k: &Kit) -> El {
    let run = &s.send.run;
    let mut right = row().gap(SPACE_SM).center_y();
    if run.is_running() {
        let requested = run.cancel_requested();
        right = right.child(
            Button::new(if requested { "Canceling" } else { "Cancel" })
                .variant(ButtonVariant::Outline)
                .disabled(requested)
                .on_press(|s: &mut AppState| dispatch(s, Input::Send(SendInput::CancelPressed)))
                .build(k)
                .key("send-cancel"),
        );
    }
    right = right.child(
        Button::new("Send")
            .variant(ButtonVariant::Submit)
            .disabled(run.is_running())
            .on_press_tx(press_send)
            .build(k)
            .key(SEND_BUTTON_KEY),
    );
    row()
        .key("send-action-row")
        .w_full()
        .gap(SPACE_MD)
        .center_y()
        .px(SPACE_LG)
        .py(SPACE_MD)
        .child(status(s, k))
        .child(right)
}

/// The status block: keyed by the run's phase, so it swaps only when the phase changes, and
/// named for assistive technology by a phrase per phase rather than by its changing numbers.
fn status(s: &AppState, k: &Kit) -> El {
    let send = &s.send;
    let issues = send.shown_issues();
    let (phase, phrase, children): (&str, String, Vec<El>) =
        if !send.run.is_running() && !issues.is_empty() {
            let line = match issues.len() {
                1 => "Fix 1 field to send".to_owned(),
                count => format!("Fix {count} fields to send"),
            };
            (
                "issues",
                line.clone(),
                vec![text(line).font(TYPE_CAPTION).color(k.c.danger.fg)],
            )
        } else {
            match &send.run {
                SendRun::Idle => idle(s, k),
                SendRun::Preparing { .. } => {
                    let (line, _) = send.run.running_words().unwrap_or_default();
                    (
                        "connecting",
                        line.clone(),
                        vec![
                            Spinner::new().build(k),
                            text(line)
                                .font(TYPE_BODY_NUM)
                                .color(k.c.text_base)
                                .truncate(),
                        ],
                    )
                }
                SendRun::Sending {
                    shown,
                    elapsed_secs,
                    target,
                    ..
                } => {
                    let (before, after) = send.run.running_words().unwrap_or_default();
                    let words = row()
                        .gap(SPACE_XS)
                        .min_w(0.0)
                        .shrink()
                        .center_y()
                        .child(text(before).font(TYPE_BODY_NUM).color(k.c.text_base))
                        .child(
                            RollingText::new("send-sent", format_thousands(*shown))
                                .value(*shown as f64)
                                .font(TYPE_BODY_NUM)
                                .color(k.c.text_base)
                                .spring(motion::CONTROL)
                                .build(k),
                        )
                        .child(
                            text(after)
                                .font(TYPE_BODY_NUM)
                                .color(k.c.text_base)
                                .truncate()
                                .shrink()
                                .min_w(0.0),
                        )
                        .child(
                            text(format!("· {}", short_duration(*elapsed_secs)))
                                .font(TYPE_BODY_NUM)
                                .color(k.c.text_muted),
                        );
                    (
                        "sending",
                        format!("Sending to {}", target.hub()),
                        vec![Spinner::new().build(k), words],
                    )
                }
                SendRun::Done {
                    summary,
                    close_error,
                    ..
                } => done(s, k, &summary.end, close_error.as_deref()),
            }
        };
    let phrase = if send.run.cancel_requested() {
        "Canceling".to_owned()
    } else {
        phrase
    };
    let children = children.into_iter().map(El::a11y_hidden);
    row()
        .key(format!("{STATUS_KEY}-{phase}"))
        .role("status")
        .label(phrase)
        .live_region(LiveRegion::Polite)
        .grow(1.0)
        .min_w(0.0)
        .gap(SPACE_SM)
        .center_y()
        .transition(
            Transition::blur_replace()
                .springs(SpringPair::new(motion::CONTROL, motion::CONTROL_EXIT)),
        )
        .children(children)
}

fn idle(s: &AppState, k: &Kit) -> (&'static str, String, Vec<El>) {
    let line = s.send.composer.summary(sidebar::hub_words(s).as_deref());
    (
        "summary",
        line.clone(),
        vec![text(line).font(TYPE_BODY).color(k.c.text_muted).truncate()],
    )
}

fn done(
    s: &AppState,
    k: &Kit,
    end: &SendEnd,
    close_error: Option<&str>,
) -> (&'static str, String, Vec<El>) {
    let line = s.send.run.outcome_line().unwrap_or_default();
    let (phase, first) = match end {
        SendEnd::Completed => (
            "done-completed",
            row()
                .gap(SPACE_SM)
                .center_y()
                .child(icon(k.glyph.check, ICON_SIZE).color(k.c.success.fg))
                .child(text(line.clone()).font(TYPE_BODY).color(k.c.text_base)),
        ),
        SendEnd::Cancelled => (
            "done-cancelled",
            row().child(text(line.clone()).font(TYPE_BODY).color(k.c.text_base)),
        ),
        SendEnd::Failed(_) => (
            "done-failed",
            row().min_w(0.0).child(
                text(line.clone())
                    .font(TYPE_BODY)
                    .color(k.c.danger.fg)
                    .wrap()
                    .selectable(true),
            ),
        ),
    };
    let mut block = col().gap(SPACE_SM).min_w(0.0).child(first);
    if let Some(error) = close_error {
        block = block.child(
            text(format!(
                "Closing the connection failed: {}",
                display_line(error.chars())
            ))
            .font(TYPE_CAPTION)
            .color(k.c.warning.fg)
            .wrap(),
        );
    }
    (phase, line, vec![block])
}
