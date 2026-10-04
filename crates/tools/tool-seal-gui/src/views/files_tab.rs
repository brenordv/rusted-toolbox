use crate::state::{self, AppState, OutputChoice, RowStatus};
use crate::views::{ViewIntent, picker};
use common_gui::tokens::{Palette, text_mono};
use common_gui::widgets::{self, BadgeKind};
use common_utils::string_utils::format_bytes_to_string;
use egui::{RichText, ScrollArea, Ui};
use shared_crypto::Direction;

/// The Files tab: pick or drop files, run them through one mode (Seal or
/// Unseal) into an output location, with per-row progress and results.
pub fn show(ui: &mut Ui, state: &mut AppState) -> Option<ViewIntent> {
    let mut intent = None;
    let palette = Palette::default();

    widgets::toolbar(ui, |ui| {
        let before = state.files.mode;
        // Locked while a job runs: a mid-run flip would revalidate the rows
        // and wipe the statuses the running batch is painting.
        ui.add_enabled_ui(!state.busy, |ui| {
            ui.selectable_value(&mut state.files.mode, Direction::Encrypt, "Seal");
            ui.selectable_value(&mut state.files.mode, Direction::Decrypt, "Unseal");
        });
        if state.files.mode != before {
            state.files.revalidate();
        }
        if widgets::ghost_button(ui, "Add files...").clicked()
            && let Some(paths) = rfd::FileDialog::new().set_title("Add files").pick_files()
        {
            intent = Some(ViewIntent::AddFiles(paths));
        }
        if ui
            .checkbox(&mut state.files.overwrite, "overwrite existing")
            .changed()
        {
            state.config_dirty = true;
        }
    });

    widgets::drop_target(ui, state.files.drop_hover, |ui| {
        ui.heading("Files");
        ui.separator();
        if state.files.rows.is_empty() {
            ui.label(
                RichText::new("drop files here, or click Add files...").color(palette.text_muted),
            );
        } else {
            ScrollArea::vertical()
                .id_salt("file-rows")
                .max_height(220.0)
                .show(ui, |ui| {
                    for row in &state.files.rows {
                        let (badge, badge_text) = badge_for(&row.status);
                        ui.horizontal(|ui| {
                            widgets::status_badge(ui, badge, &badge_text);
                            ui.label(
                                RichText::new(row.path.display().to_string()).font(text_mono()),
                            );
                            ui.label(
                                RichText::new(format_bytes_to_string(row.size))
                                    .color(palette.text_muted),
                            );
                        });
                        match &row.status {
                            RowStatus::Invalid(message) | RowStatus::Failed(message) => {
                                ui.colored_label(palette.error, message);
                            }
                            RowStatus::Pending if row.output_exists && !state.files.overwrite => {
                                ui.colored_label(
                                    palette.warning,
                                    "output exists; this row will be skipped",
                                );
                            }
                            _ => {}
                        }
                    }
                });
        }
    });

    match state.files.mode {
        Direction::Encrypt => widgets::section(ui, "Recipients", |ui| {
            picker::recipient_picker(ui, &mut state.files.picker, &state.book, "files");
        }),
        Direction::Decrypt => widgets::section(ui, "Identity", |ui| {
            picker::identity_row(ui, &mut state.identity_path, &mut state.config_dirty);
        }),
    }

    widgets::section(ui, "Output", |ui| {
        let before_choice = state.files.out_choice;
        let before_dir = state.files.out_dir.clone();
        ui.horizontal(|ui| {
            ui.radio_value(
                &mut state.files.out_choice,
                OutputChoice::NextToInput,
                "Next to each input",
            );
            ui.radio_value(&mut state.files.out_choice, OutputChoice::Folder, "Folder:");
            match state.files.out_dir.as_ref() {
                Some(dir) => {
                    ui.label(RichText::new(dir.display().to_string()).font(text_mono()));
                }
                None => {
                    ui.label(RichText::new("none chosen").color(palette.text_muted));
                }
            }
            if widgets::ghost_button(ui, "Choose...").clicked()
                && let Some(dir) = rfd::FileDialog::new()
                    .set_title("Choose output folder")
                    .pick_folder()
            {
                state.files.out_dir = Some(dir);
                state.files.out_choice = OutputChoice::Folder;
                state.config_dirty = true;
            }
        });
        if state.files.out_choice != before_choice || state.files.out_dir != before_dir {
            state.files.exists_refresh_pending = true;
        }
    });

    let job = state::files_job(state);
    let label = match state.files.mode {
        Direction::Encrypt => "Seal files",
        Direction::Decrypt => "Unseal files",
    };
    let can_run = !state.busy && !state.worker_gone && job.is_ok();
    let mut response = ui
        .add_enabled_ui(can_run, |ui| widgets::primary_button(ui, label))
        .inner;
    if let Err(reason) = &job {
        response = response.on_disabled_hover_text(reason.as_str());
    }
    if response.clicked()
        && let Ok(job) = job
    {
        intent = Some(ViewIntent::Submit(job));
    }

    intent
}

fn badge_for(status: &RowStatus) -> (BadgeKind, String) {
    match status {
        RowStatus::Pending => (BadgeKind::Neutral, "pending".to_string()),
        RowStatus::Invalid(_) => (BadgeKind::Err, "invalid".to_string()),
        RowStatus::Running => (BadgeKind::Neutral, "running".to_string()),
        RowStatus::Done(size) => (
            BadgeKind::Ok,
            format!("done ({})", format_bytes_to_string(*size)),
        ),
        RowStatus::Failed(_) => (BadgeKind::Err, "failed".to_string()),
        RowStatus::Skipped => (BadgeKind::Warn, "skipped".to_string()),
    }
}
