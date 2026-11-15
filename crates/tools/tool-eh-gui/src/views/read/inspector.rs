//! The inspector: the selected message's metadata and body, copy, and export.

use super::list::copy_selected_body;
use crate::app::dispatch;
use crate::design::parts;
use crate::design::tokens::{SPACE_LG, SPACE_MD, SPACE_SM, TYPE_MONO, TYPE_TITLE};
use crate::read::inspector::{
    ExportPhase, FeedbackKind, export_file_name, message_json, shown_body,
};
use crate::read::rows::display_line;
use crate::read::{ReadInput, Selected};
use crate::state::{AppState, Input};
use chrono::Local;
use rune::{El, FileDialogOptions, ScrollSpec, Tx, col, el, row, text};
use rune_kit::{
    Button, ButtonVariant, IconButton, IconButtonVariant, Kit, ScrollBar, SegmentedControl, Switch,
};
use shared_eventhub_core::models::BodyKind;
use shared_eventhub_persistence::export::ExportFormat;

const FORMATS: [ExportFormat; 3] = [ExportFormat::Txt, ExportFormat::Csv, ExportFormat::Json];
const FORMAT_LABELS: [&str; 3] = ["txt", "csv", "json"];

/// The inspector pane's content: the selected message, or nothing while none is selected (the
/// pane is closed then).
pub fn inspector(s: &AppState, k: &Kit) -> El {
    let Some(selected) = &s.read.selected else {
        return el().key("inspector-empty");
    };
    let message = &selected.message;
    let enqueued = message
        .enqueued_time
        .map_or("No enqueue time".to_owned(), |at| {
            format!(
                "{} UTC · {} local",
                at.format("%Y-%m-%d %H:%M:%S%.3f"),
                at.with_timezone(&Local).format("%H:%M:%S%.3f")
            )
        });
    let close = IconButton::new(k.glyph.close)
        .variant(IconButtonVariant::Ghost)
        .label("Close")
        .on_press(|s: &mut AppState| dispatch(s, Input::Read(ReadInput::ClearSelection)))
        .build(k)
        .key("inspector-close");
    let title = text(format!(
        "Partition {} · #{}",
        display_line(message.key.partition_id.chars()),
        message.key.sequence_number
    ))
    .font(TYPE_TITLE)
    .color(k.c.text_base)
    .grow(1.0);
    let mut pane = col()
        .key("inspector")
        .w_full()
        .gap(SPACE_MD)
        .padding(SPACE_LG)
        .child(
            row()
                .w_full()
                .gap(SPACE_SM)
                .center_y()
                .child(title)
                .child(close),
        )
        .child(
            text(enqueued)
                .font(TYPE_MONO)
                .color(k.c.text_muted)
                .selectable(true),
        );
    match message.body_kind {
        BodyKind::Utf8 => {}
        BodyKind::LossyUtf8 => {
            pane = pane.child(parts::warning_badge(
                k,
                "Not valid UTF-8; shown with replacements",
            ));
        }
        BodyKind::Missing => pane = pane.child(parts::warning_badge(k, "No binary body")),
    }
    let pane = pane
        .child(metadata(k, selected))
        .child(actions(s, k, selected))
        .child(export(s, k))
        .child(body(k, &message.body));
    col()
        .key("inspector-scroll")
        .w_full()
        .h_full()
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(pane)
        .scroll_bar(k)
}

fn metadata(k: &Kit, selected: &Selected) -> El {
    let message = &selected.message;
    let shown = |value: &Option<String>| {
        value
            .as_deref()
            .map_or("--".to_owned(), |value| display_line(value.chars()))
    };
    let mut rows = vec![
        parts::detail_row(k, "Key", &selected.key_str),
        parts::detail_row(k, "Offset", &shown(&message.offset)),
        parts::detail_row(k, "Partition key", &shown(&message.partition_key)),
        parts::detail_row(k, "Message id", &shown(&message.message_id)),
        parts::detail_row(k, "Correlation id", &shown(&message.correlation_id)),
        parts::detail_row(k, "Content type", &shown(&message.content_type)),
        parts::detail_row(
            k,
            "Processed at",
            &message
                .processed_at
                .format("%Y-%m-%d %H:%M:%S%.3f UTC")
                .to_string(),
        ),
    ];
    for (name, value) in &message.properties {
        rows.push(parts::detail_row(
            k,
            &display_line(name.chars()),
            &display_line(value.chars()),
        ));
    }
    parts::metadata_card(k, rows)
}

fn actions(s: &AppState, k: &Kit, selected: &Selected) -> El {
    let shows = |kind| {
        s.read
            .feedback
            .as_ref()
            .is_some_and(|feedback| feedback.shows(&selected.key, kind))
    };
    let label =
        |kind, idle: &'static str, done: &'static str| if shows(kind) { done } else { idle };
    row()
        .gap(SPACE_SM)
        .child(
            Button::new(label(FeedbackKind::CopiedBody, "Copy body", "Copied"))
                .variant(ButtonVariant::Neutral)
                .icon(if shows(FeedbackKind::CopiedBody) {
                    k.glyph.check
                } else {
                    k.glyph.copy
                })
                .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                    copy_selected_body(s, tx);
                })
                .build(k)
                .key("copy-body"),
        )
        .child(
            Button::new(label(FeedbackKind::CopiedJson, "Copy as JSON", "Copied"))
                .variant(ButtonVariant::Neutral)
                .on_press_tx(copy_json)
                .build(k)
                .key("copy-json"),
        )
}

fn copy_json(s: &mut AppState, tx: &mut Tx) {
    let Some(selected) = &s.read.selected else {
        return;
    };
    let input = match message_json(&selected.message) {
        Ok(json) => {
            tx.copy(json);
            ReadInput::Copied {
                kind: FeedbackKind::CopiedJson,
                now: chrono::Utc::now(),
            }
        }
        Err(error) => ReadInput::CopyFailed(error),
    };
    dispatch(s, Input::Read(input));
}

fn export(s: &AppState, k: &Kit) -> El {
    let read = &s.read;
    let index = FORMATS
        .iter()
        .position(|format| *format == read.export_format);
    let exported = read.selected.as_ref().is_some_and(|selected| {
        read.feedback
            .as_ref()
            .is_some_and(|feedback| feedback.shows(&selected.key, FeedbackKind::Exported))
    });
    let label = match (&read.export, exported) {
        (ExportPhase::Running { .. }, _) => "Exporting",
        (ExportPhase::Idle | ExportPhase::Picking { .. }, true) => "Exported",
        (ExportPhase::Idle | ExportPhase::Picking { .. }, false) => "Export…",
    };
    let mut group = col()
        .key("export-one")
        .gap(SPACE_SM)
        .w_full()
        .child(
            row()
                .gap(SPACE_SM)
                .center_y()
                .child(
                    SegmentedControl::new(&FORMAT_LABELS, index)
                        .label("Export format")
                        .fit()
                        .on_pick(|s: &mut AppState, index: usize, _| {
                            if let Some(format) = FORMATS.get(index) {
                                s.read.export_format = *format;
                            }
                        })
                        .build(k),
                )
                .child(
                    Switch::new(read.export_metadata)
                        .label("Metadata")
                        .on_toggle(|s: &mut AppState| {
                            s.read.export_metadata = !s.read.export_metadata
                        })
                        .build(k),
                ),
        )
        .child(
            Button::new(label)
                .variant(ButtonVariant::Outline)
                .disabled(!read.export.is_idle())
                .on_press_tx(open_export_dialog)
                .build(k)
                .key("export-message"),
        );
    if read.export_format == ExportFormat::Csv {
        group = group.child(parts::caption(
            k,
            "Spreadsheet apps may run cells that start with =, +, - or @ as formulas.",
        ));
    }
    group
}

/// Starts an export of the selection and opens the save dialog for it; does nothing while an
/// export is already picking or writing.
fn open_export_dialog(s: &mut AppState, tx: &mut Tx) {
    if !s.read.export.is_idle() {
        return;
    }
    dispatch(s, Input::Read(ReadInput::ExportPressed));
    let (session, key) = match &s.read.export {
        ExportPhase::Picking { session, key, .. } => (*session, key.clone()),
        ExportPhase::Idle | ExportPhase::Running { .. } => return,
    };
    let options = FileDialogOptions {
        title: "Export the message".to_owned(),
        file_name: Some(export_file_name(&key, s.read.export_format)),
        ..FileDialogOptions::default()
    };
    tx.save_file(options, move |s: &mut AppState, answer, _| {
        let answer = answer.map_err(|error| error.to_string());
        dispatch(
            s,
            Input::Read(ReadInput::ExportAnswered {
                session,
                key,
                answer,
            }),
        );
    });
}

fn body(k: &Kit, body: &str) -> El {
    let (shown, caption) = shown_body(body);
    let mut area = col().key("inspector-body").w_full().gap(SPACE_SM);
    if let Some(caption) = caption {
        area = area.child(parts::caption(k, &caption));
    }
    area.child(parts::body_text(k, shown))
}
