//! The inspector both sections share: the selected message's metadata and body, copy, and
//! export.

use super::message_list::{clear_selection_input, copy_selected_body};
use crate::app::dispatch;
use crate::design::parts::{self, LabelSide};
use crate::design::tokens::{SPACE_LG, SPACE_MD, SPACE_SM, TYPE_MONO, TYPE_TITLE};
use crate::read::Selected;
use crate::read::inspector::{
    ExportPhase, FeedbackKind, InspectorInput, InspectorState, export_file_name, message_json,
    shown_body,
};
use crate::read::rows::display_line;
use crate::state::{AppState, Input, ListSection};
use chrono::Local;
use eh_core::models::BodyKind;
use eh_persistence::export::ExportFormat;
use rune::{El, FileDialogOptions, ScrollSpec, Tx, col, el, row, text};
use rune_kit::{
    Button, ButtonVariant, IconButton, IconButtonVariant, Kit, ScrollBar, SegmentedControl,
};

/// The export formats in picker order, with their labels.
pub const FORMATS: [ExportFormat; 3] = [ExportFormat::Txt, ExportFormat::Csv, ExportFormat::Json];
pub const FORMAT_LABELS: [&str; 3] = ["txt", "csv", "json"];
/// The caption under a csv pick.
pub const CSV_CAPTION: &str =
    "Spreadsheet apps may run cells that start with =, +, - or @ as formulas.";

/// The Read inspector's Metadata switch key.
pub const READ_METADATA_KEY: &str = "read-metadata";
/// The Export inspector's Metadata switch key.
pub const EXPORT_METADATA_KEY: &str = "export-metadata";

/// The key of `section`'s inspector Metadata switch.
fn metadata_key(section: ListSection) -> &'static str {
    match section {
        ListSection::Read => READ_METADATA_KEY,
        ListSection::Export => EXPORT_METADATA_KEY,
    }
}

/// `section`'s inspector.
pub fn inspector_of(s: &AppState, section: ListSection) -> &InspectorState {
    match section {
        ListSection::Read => &s.read.inspector,
        ListSection::Export => &s.archive.inspector,
    }
}

fn inspector_of_mut(s: &mut AppState, section: ListSection) -> &mut InspectorState {
    match section {
        ListSection::Read => &mut s.read.inspector,
        ListSection::Export => &mut s.archive.inspector,
    }
}

/// The inspector pane's content: `section`'s selected message, or nothing while none is
/// selected (the pane is closed then).
pub fn inspector(s: &AppState, k: &Kit, section: ListSection) -> El {
    let state = inspector_of(s, section);
    let Some(selected) = &state.selected else {
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
        .on_press(move |s: &mut AppState| dispatch(s, clear_selection_input(section)))
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
                .wrap()
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
        .child(actions(state, k, selected, section))
        .child(export(state, k, section))
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

fn actions(state: &InspectorState, k: &Kit, selected: &Selected, section: ListSection) -> El {
    let shows = |kind| {
        state
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
                .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                    copy_selected_body(s, tx, section);
                })
                .build(k)
                .key("copy-body"),
        )
        .child(
            Button::new(label(FeedbackKind::CopiedJson, "Copy as JSON", "Copied"))
                .variant(ButtonVariant::Neutral)
                .on_press_tx(move |s: &mut AppState, tx: &mut Tx| copy_json(s, tx, section))
                .build(k)
                .key("copy-json"),
        )
}

fn copy_json(s: &mut AppState, tx: &mut Tx, section: ListSection) {
    let Some(selected) = &inspector_of(s, section).selected else {
        return;
    };
    let input = match message_json(&selected.message) {
        Ok(json) => {
            tx.copy(json);
            InspectorInput::Copied {
                kind: FeedbackKind::CopiedJson,
                now: chrono::Utc::now(),
            }
        }
        Err(error) => InspectorInput::CopyFailed(error),
    };
    dispatch(s, Input::inspector(section, input));
}

fn export(state: &InspectorState, k: &Kit, section: ListSection) -> El {
    let index = FORMATS.iter().position(|format| *format == state.format);
    let exported = state.selected.as_ref().is_some_and(|selected| {
        state
            .feedback
            .as_ref()
            .is_some_and(|feedback| feedback.shows(&selected.key, FeedbackKind::Exported))
    });
    let label = match (&state.export, exported) {
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
                        .on_pick(move |s: &mut AppState, index: usize, _| {
                            if let Some(format) = FORMATS.get(index) {
                                inspector_of_mut(s, section).format = *format;
                            }
                        })
                        .build(k),
                )
                .child(parts::labeled_switch(
                    k,
                    metadata_key(section),
                    "Metadata",
                    LabelSide::Leading,
                    state.metadata,
                    move |s: &mut AppState| {
                        let inspector = inspector_of_mut(s, section);
                        inspector.metadata = !inspector.metadata;
                    },
                )),
        )
        .child(
            Button::new(label)
                .variant(ButtonVariant::Outline)
                .disabled(!state.export.is_idle())
                .on_press_tx(move |s: &mut AppState, tx: &mut Tx| {
                    open_export_dialog(s, tx, section);
                })
                .build(k)
                .key("export-message"),
        );
    if state.format == ExportFormat::Csv {
        group = group.child(parts::caption(k, CSV_CAPTION));
    }
    group
}

/// Starts an export of the selection and opens the save dialog for it; does nothing while an
/// export is already picking or writing.
fn open_export_dialog(s: &mut AppState, tx: &mut Tx, section: ListSection) {
    if !inspector_of(s, section).export.is_idle() {
        return;
    }
    dispatch(s, Input::inspector(section, InspectorInput::ExportPressed));
    let state = inspector_of(s, section);
    let (owner, key) = match &state.export {
        ExportPhase::Picking { owner, key, .. } => (*owner, key.clone()),
        ExportPhase::Idle | ExportPhase::Running { .. } => return,
    };
    let options = FileDialogOptions {
        title: "Export the message".to_owned(),
        file_name: Some(export_file_name(&key, state.format)),
        ..FileDialogOptions::default()
    };
    tx.save_file(options, move |s: &mut AppState, answer, _| {
        let answer = answer.map_err(|error| error.to_string());
        dispatch(
            s,
            Input::inspector(
                section,
                InspectorInput::ExportAnswered { owner, key, answer },
            ),
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
