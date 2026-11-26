//! The Send section: the body editor over the action row that shows the run.

pub mod sidebar;
pub mod template;

use super::details;
use crate::app::dispatch;
use crate::design::tokens::{
    CAPTION_ROW_HEIGHT, ICON_SIZE, SPACE_LG, SPACE_SM, SPACE_XS, TYPE_BODY, TYPE_BODY_NUM,
    TYPE_CAPTION, TYPE_LABEL,
};
use crate::design::{motion, parts};
use crate::read::rows::display_line;
use crate::send::SendInput;
use crate::send::compose::{BODY_CAP, SendField};
use crate::send::run::SendRun;
use crate::state::{AppState, Input, Section, error_preview};
use crate::views::read::short_duration;
use common_gui_rune::keys::{area_chord, is_chord, type_newline};
use common_gui_rune::parts::action_row as fixed_action_row;
use common_utils::string_utils::{format_bytes_to_string, format_thousands};
use eh_core::producer::SendEnd;
use rune::{El, KeyEvent, LiveRegion, Transition, Tx, col, icon, row, spacer, text};
use rune_kit::{Button, ButtonVariant, Kit, RollingText, Spinner};

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
        SendField::Properties => sidebar::PROPERTIES_KEY,
        SendField::Body => BODY_KEY,
    }
}

/// The element scrolled into view when focus moves to `field`: the field's group with its issue,
/// the destination block for the target, or the body editor.
fn issue_block_key(field: SendField) -> String {
    match field {
        SendField::Target => "send-destination".to_owned(),
        SendField::PartitionKey
        | SendField::Count
        | SendField::Interval
        | SendField::Properties => format!("{}-group", field_key(field)),
        SendField::Body => BODY_KEY.to_owned(),
    }
}

/// Presses Send and moves focus to the first part with an issue, if the press found any,
/// scrolling the sidebar until that part and its issue show.
pub fn press_send(s: &mut AppState, tx: &mut Tx) {
    dispatch(s, Input::Send(SendInput::SendPressed));
    if let Some(field) = s.send.take_focus() {
        tx.focus(field_key(field), true);
        tx.scroll_into_view(issue_block_key(field));
    }
}

/// The shortcuts that work while typing in the Send section and with nothing focused, none of
/// them while a sheet or the error details are open: `mod+enter` sends while the Send section
/// shows, and `mod+1..3` pick a section as a fresh press. True when `key` was one of them and
/// acted.
pub fn send_keys(s: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if s.sheet.is_some() || s.details().is_some() {
        return false;
    }
    if is_chord("mod+enter", key, tx) {
        if s.section != Section::Send {
            return false;
        }
        press_send(s, tx);
        return true;
    }
    match area_chord(Section::ALL.len(), key, tx).and_then(|index| Section::ALL.get(index)) {
        Some(section) => {
            dispatch(s, Input::SectionPicked(*section));
            true
        }
        None => false,
    }
}

/// The body editor's keys: the Send shortcuts, and Return typing a newline (the Send section has
/// no default button), after which the template follows the new text (the field's change hook
/// does not run for this edit).
fn body_keys(s: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if send_keys(s, key, tx) {
        return true;
    }
    let composer = &mut s.send.composer;
    let typed = type_newline(&mut composer.body, key);
    if typed {
        composer.refresh_template();
    }
    typed
}

pub fn content(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> El {
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
        .h(CAPTION_ROW_HEIGHT)
        .gap(SPACE_LG)
        .center_y()
        .px(SPACE_LG)
        .mt(SPACE_LG)
        .mb(SPACE_SM)
        .child(text("Event body").font(TYPE_LABEL).color(k.c.text_base))
        .child(template::switch_controls(s, k, layer))
        .child(spacer())
        .child(size);
    let editor = parts::body_editor(
        k,
        BODY_KEY,
        &send.composer.body,
        issues.body.is_some(),
        |s: &mut AppState| &mut s.send.composer.body,
        body_keys,
        |s: &mut AppState, _: &mut Tx| s.send.composer.refresh_template(),
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
        body = body.child(parts::issue(k, &display_line(issue.chars())));
    }
    if let Some(sample) = template::sample(s, k) {
        body = body.child(parts::reveal(sample));
    }
    col()
        .key("send-content")
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(caption_row)
        .child(body)
        .child(action_row(s, k))
}

/// The fixed row under the body: the run's status, Cancel while it runs, and Send.
fn action_row(s: &AppState, k: &Kit) -> El {
    let run = &s.send.run;
    let mut buttons = Vec::new();
    if run.is_running() {
        let requested = run.cancel_requested();
        buttons.push(
            Button::new(if requested { "Canceling" } else { "Cancel" })
                .variant(ButtonVariant::Outline)
                .disabled(requested)
                .on_press(|s: &mut AppState| dispatch(s, Input::Send(SendInput::CancelPressed)))
                .build(k)
                .key("send-cancel"),
        );
    }
    if run_failure_cut(run) {
        buttons.push(details::button(k, "send-details", open_send_failure));
    }
    buttons.push(
        Button::new("Send")
            .variant(ButtonVariant::Submit)
            .disabled(run.is_running())
            .on_press_tx(press_send)
            .build(k)
            .key(SEND_BUTTON_KEY),
    );
    fixed_action_row(k, "send-action-row", Some(status(s, k)), buttons)
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
        .transition(Transition::blur_replace().springs(motion::CONTROL_PAIR))
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

/// The prefix of a close error, on screen and in its details.
const CLOSE_FAILED: &str = "Closing the connection failed: ";

/// Whether an ended run's failure or close error was cut short on the action row.
fn run_failure_cut(run: &SendRun) -> bool {
    match run {
        SendRun::Done {
            summary,
            close_error,
            ..
        } => {
            let failed = match &summary.end {
                SendEnd::Failed(_) => run
                    .outcome_line()
                    .is_some_and(|line| error_preview(&line).cut),
                SendEnd::Completed | SendEnd::Cancelled => false,
            };
            failed
                || close_error
                    .as_deref()
                    .is_some_and(|error| error_preview(error).cut)
        }
        SendRun::Idle | SendRun::Preparing { .. } | SendRun::Sending { .. } => false,
    }
}

/// Opens the ended run's outcome and close error in full; nothing once the run moved on.
fn open_send_failure(s: &mut AppState, _: &mut Tx) {
    let run = &s.send.run;
    let (end, close_error) = match run {
        SendRun::Done {
            summary,
            close_error,
            ..
        } => (&summary.end, close_error.as_deref()),
        SendRun::Idle | SendRun::Preparing { .. } | SendRun::Sending { .. } => return,
    };
    let title = match end {
        SendEnd::Failed(_) => "The send failed",
        SendEnd::Completed | SendEnd::Cancelled => "Closing the connection failed",
    };
    let mut text = run.outcome_line().unwrap_or_default();
    if let Some(error) = close_error {
        text = format!("{text}\n\n{CLOSE_FAILED}{error}");
    }
    details::show(s, title, &text);
}

fn done(
    s: &AppState,
    k: &Kit,
    end: &SendEnd,
    close_error: Option<&str>,
) -> (&'static str, String, Vec<El>) {
    let line = s.send.run.outcome_line().unwrap_or_default();
    let shown = error_preview(&line).shown;
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
                text(shown.clone())
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
            text(format!("{CLOSE_FAILED}{}", error_preview(error).shown))
                .font(TYPE_CAPTION)
                .color(k.c.warning.fg)
                .wrap(),
        );
    }
    (phase, shown, vec![block])
}
