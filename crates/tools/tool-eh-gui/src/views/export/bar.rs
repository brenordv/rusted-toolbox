//! The bulk export bar: export every match of the last search, with its options, its live
//! counts, Cancel, and the result.

use crate::app::dispatch;
use crate::archive::ArchiveInput;
use crate::archive::bulk::{BulkExport, counts_line, run_failed};
use crate::archive::results::Results;
use crate::design::parts;
use crate::design::tokens::{SPACE_MD, SPACE_SM, SPACE_XS};
use crate::state::{AppState, Input};
use crate::views::inspector::{CSV_CAPTION, FORMAT_LABELS, FORMATS};
use common_cli::app_logger::FileLogging;
use rune::{El, FileDialogOptions, Tx, col, row};
use rune_kit::{Button, ButtonVariant, Kit, Placement, SegmentedControl, Spinner, Switch, popover};
use shared_eventhub_persistence::export::ExportFormat;

/// The bar, when it shows: while an export runs or after it ended, and with results to export.
pub fn bar(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> Option<El> {
    let children = match &s.archive.bulk {
        BulkExport::Idle => {
            let has_rows = match &s.archive.results {
                Results::Shown { rows, .. } => !rows.is_empty(),
                Results::NotSearched | Results::Loading { .. } | Results::Unavailable { .. } => {
                    false
                }
            };
            if !has_rows {
                return None;
            }
            idle(s, k, layer)
        }
        BulkExport::Running {
            counts,
            cancel_requested,
            ..
        } => vec![
            Spinner::new().build(k),
            parts::live_numbers(k, &counts_line(counts)),
            Button::new(if *cancel_requested {
                "Canceling"
            } else {
                "Cancel"
            })
            .variant(ButtonVariant::Outline)
            .disabled(*cancel_requested)
            .on_press(|s: &mut AppState| dispatch(s, Input::Archive(ArchiveInput::CancelBulk)))
            .build(k)
            .key("bulk-cancel"),
        ],
        BulkExport::Done { result, .. } => {
            let mut summary = col().gap(SPACE_SM);
            match result {
                Ok(outcome) => {
                    let mut line = counts_line(outcome);
                    if outcome.cancelled {
                        line = format!("Canceled · {line}");
                    }
                    summary = summary.child(parts::live_numbers(k, &line));
                    if run_failed(result)
                        && let Some(error) = &outcome.first_error
                    {
                        summary = summary.child(parts::caption(k, error));
                        match &s.file_logging {
                            FileLogging::Writing(_) => {
                                summary = summary
                                    .child(parts::caption(k, "Details are in the log file."));
                            }
                            FileLogging::Off | FileLogging::Failed { .. } => {}
                        }
                    }
                }
                Err(error) => {
                    summary =
                        summary.child(parts::caption(k, &format!("The export stopped: {error}")));
                }
            }
            vec![
                summary,
                Button::new("Done")
                    .variant(ButtonVariant::Neutral)
                    .on_press(|s: &mut AppState| {
                        dispatch(s, Input::Archive(ArchiveInput::DismissBulk));
                    })
                    .build(k)
                    .key("bulk-done"),
            ]
        }
    };
    Some(parts::floating_bar(k, "bulk-bar", children).max_w(720.0))
}

/// The element the options popover anchors to.
const OPTIONS_ANCHOR: &str = "bulk-options-trigger";

fn close_options(s: &mut AppState) {
    s.archive.options_open = false;
}

fn idle(s: &AppState, k: &Kit, layer: &mut Vec<El>) -> Vec<El> {
    let open = s.archive.options_open;
    let trigger = row().key(OPTIONS_ANCHOR).child(
        Button::new("Options")
            .variant(ButtonVariant::Ghost)
            .expanded(open)
            .on_press(|s: &mut AppState| s.archive.options_open = !s.archive.options_open)
            .build(k)
            .key("bulk-options"),
    );
    if open {
        layer.push(rune_kit::scrim("bulk-options-scrim", close_options));
        let panel = parts::form_popover(
            k,
            OPTIONS_ANCHOR,
            "Export options",
            options(s, k),
            close_options,
        );
        layer.push(popover::anchored(
            k,
            panel,
            OPTIONS_ANCHOR,
            Placement::Top,
            SPACE_XS,
        ));
    }
    vec![
        trigger,
        Button::new("Export all matches")
            .variant(ButtonVariant::Submit)
            .on_press(|s: &mut AppState| {
                s.archive.options_open = false;
                dispatch(s, Input::Archive(ArchiveInput::BulkExportPressed));
            })
            .build(k)
            .key("bulk-export"),
    ]
}

fn options(s: &AppState, k: &Kit) -> El {
    let options = &s.archive.options;
    let index = FORMATS.iter().position(|format| *format == options.format);
    let switch = |label: &str, on: bool, toggle: fn(&mut AppState)| {
        Switch::new(on).label(label).on_toggle(toggle).build(k)
    };
    let destination = options.destination(&s.form.data_paths());
    let mut panel = col()
        .key("bulk-options-panel")
        .gap(SPACE_MD)
        .w_full()
        .child(
            SegmentedControl::new(&FORMAT_LABELS, index)
                .label("Format")
                .fit()
                .on_pick(|s: &mut AppState, index: usize, _| {
                    if let Some(format) = FORMATS.get(index) {
                        s.archive.options.format = *format;
                    }
                })
                .build(k),
        );
    if options.format == ExportFormat::Csv {
        panel = panel.child(parts::caption(k, CSV_CAPTION));
    }
    panel
        .child(switch("Include metadata", options.metadata, |s| {
            s.archive.options.metadata = !s.archive.options.metadata;
        }))
        .child(switch("One file per month", options.condense, |s| {
            s.archive.options.condense = !s.archive.options.condense;
        }))
        .child(switch("Local time", options.local_time, |s| {
            s.archive.options.local_time = !s.archive.options.local_time;
        }))
        .child(switch(
            "Skip already exported",
            options.skip_exported,
            |s| {
                s.archive.options.skip_exported = !s.archive.options.skip_exported;
            },
        ))
        .child(parts::caption(
            k,
            "A message exported before, in any format or folder, is skipped unless its file is \
             gone.",
        ))
        .child(
            row()
                .w_full()
                .gap(SPACE_SM)
                .items_end()
                .child(
                    parts::detail_row(k, "Destination", &destination.display().to_string())
                        .grow(1.0)
                        .min_w(0.0),
                )
                .child(
                    Button::new("Change")
                        .variant(ButtonVariant::Outline)
                        .on_press_tx(pick_destination)
                        .build(k)
                        .key("bulk-destination"),
                ),
        )
}

fn pick_destination(_: &mut AppState, tx: &mut Tx) {
    let options = FileDialogOptions {
        title: "Export the matches into".to_owned(),
        ..FileDialogOptions::default()
    };
    tx.open_folder(options, |s: &mut AppState, picked, _| match picked {
        Ok(Some(folder)) => dispatch(s, Input::Archive(ArchiveInput::DestinationPicked(folder))),
        Ok(None) => {}
        Err(error) => dispatch(
            s,
            Input::Archive(ArchiveInput::DestinationFailed(error.to_string())),
        ),
    });
}
