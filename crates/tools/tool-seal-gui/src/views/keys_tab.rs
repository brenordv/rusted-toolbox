use crate::state::{self, AppState};
use crate::views::ViewIntent;
use crate::worker::JobRequest;
use common_gui::tokens::{Palette, text_mono};
use common_gui::widgets::{self, Toast, ToastKind};
use egui::{RichText, ScrollArea, TextEdit, Ui};

/// The Keys tab: generate a key pair (the secret goes straight to a file the
/// user picks; only the public key is ever shown) and manage the recipient
/// book.
pub fn show(ui: &mut Ui, state: &mut AppState) -> Option<ViewIntent> {
    let mut intent = None;
    let palette = Palette::default();

    widgets::section(ui, "Generate", |ui| {
        let can_generate = !state.busy && !state.worker_gone;
        let clicked = ui
            .add_enabled_ui(can_generate, |ui| {
                widgets::primary_button(ui, "Generate key pair")
            })
            .inner
            .clicked();
        if clicked {
            state.keys.disarm();
            // The dialog's own replace-existing confirmation is the
            // overwrite guard; a cancel means do nothing.
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Save identity file")
                .set_file_name("seal-identity.txt")
                .add_filter("age identity", &["txt"])
                .save_file()
            {
                state.keys.saved_identity_path = Some(path.clone());
                intent = Some(ViewIntent::Submit(JobRequest::Keygen { path }));
            }
        }

        if let Some(public_key) = state.keys.generated_public_key.as_deref() {
            // An immutable &str buffer: selectable and copyable, but every
            // edit is a no-op.
            let mut shown = public_key;
            ui.add(
                TextEdit::singleline(&mut shown)
                    .font(text_mono())
                    .desired_width(f32::INFINITY),
            );
        }
        if let Some(public_key) = state.keys.generated_public_key.clone() {
            ui.horizontal(|ui| {
                if widgets::ghost_button(ui, "Copy").clicked() {
                    state.keys.disarm();
                    ui.ctx().copy_text(public_key.clone());
                    state.toasts.push(Toast::new(
                        ToastKind::Ok,
                        "public key copied",
                        state::TOAST_TTL,
                    ));
                }
                if widgets::ghost_button(ui, "Add to recipient book").clicked() {
                    state.keys.disarm();
                    state.keys.add_key = public_key;
                    state.keys.add_label = state
                        .keys
                        .saved_identity_path
                        .as_ref()
                        .and_then(|path| path.file_stem())
                        .map(|stem| stem.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    state.keys.add_error = None;
                }
            });
        }
    });

    widgets::section(ui, "Recipient book", |ui| {
        if state.book.is_empty() {
            ui.label(RichText::new("no saved recipients yet").color(palette.text_muted));
        } else {
            let mut delete_now = None;
            let mut copy_now = None;
            ScrollArea::vertical()
                .id_salt("recipient-book-keys")
                .max_height(200.0)
                .show(ui, |ui| {
                    for (index, entry) in state.book.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(entry.label.as_str());
                            ui.label(
                                RichText::new(state::truncated_key(&entry.public_key))
                                    .font(text_mono())
                                    .color(palette.text_muted),
                            );
                            if widgets::ghost_button(ui, "Copy").clicked() {
                                copy_now = Some(index);
                            }
                            let armed = state.keys.confirm_delete == Some(index);
                            let delete = if armed {
                                widgets::danger_button(ui, "Confirm delete")
                            } else {
                                widgets::ghost_button(ui, "Delete")
                            };
                            if delete.clicked() && state.keys.press_delete(index) {
                                delete_now = Some(index);
                            }
                        });
                    }
                });
            if let Some(index) = copy_now {
                state.keys.disarm();
                ui.ctx().copy_text(state.book[index].public_key.clone());
                state.toasts.push(Toast::new(
                    ToastKind::Ok,
                    "public key copied",
                    state::TOAST_TTL,
                ));
            }
            if let Some(index) = delete_now {
                state.book.remove(index);
                state.config_dirty = true;
            }
        }

        ui.separator();
        widgets::form_row(ui, "Label", |ui| {
            ui.add(TextEdit::singleline(&mut state.keys.add_label).desired_width(f32::INFINITY));
        });
        widgets::form_row(ui, "Public key", |ui| {
            let response = ui.add(
                TextEdit::singleline(&mut state.keys.add_key)
                    .hint_text("age1...")
                    .font(text_mono())
                    .desired_width(f32::INFINITY),
            );
            if response.changed() {
                state.keys.add_error = None;
            }
        });
        if let Some(error) = &state.keys.add_error {
            ui.colored_label(palette.error, error);
        }
        if widgets::primary_button(ui, "Add").clicked() {
            state.keys.disarm();
            match state::add_book_entry(&mut state.book, &state.keys.add_label, &state.keys.add_key)
            {
                Ok(()) => {
                    state.keys.add_label.clear();
                    state.keys.add_key.clear();
                    state.keys.add_error = None;
                    state.config_dirty = true;
                }
                Err(message) => state.keys.add_error = Some(message),
            }
        }
    });

    intent
}
