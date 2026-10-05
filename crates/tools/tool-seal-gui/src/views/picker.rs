use crate::config::RecipientEntry;
use crate::state::{self, RecipientPicker};
use common_gui::tokens::{Palette, text_mono};
use common_gui::widgets::{form_row, ghost_button};
use egui::{RichText, ScrollArea, TextEdit, Ui};
use std::path::PathBuf;

/// Renders the shared recipient picker: the book as a checkbox list plus a
/// free-text one-off key with inline validation. `salt` keeps the two tabs'
/// scroll areas apart.
pub fn recipient_picker(
    ui: &mut Ui,
    picker: &mut RecipientPicker,
    book: &[RecipientEntry],
    salt: &str,
) {
    picker.sync_with_book(book.len());
    let palette = Palette::default();

    if book.is_empty() {
        ui.label(
            RichText::new("no saved recipients yet; add them in the Keys tab")
                .color(palette.text_muted),
        );
    } else {
        ScrollArea::vertical()
            .id_salt(format!("recipient-book-{salt}"))
            .max_height(160.0)
            .show(ui, |ui| {
                for (entry, selected) in book.iter().zip(picker.selected.iter_mut()) {
                    ui.horizontal(|ui| {
                        ui.checkbox(selected, entry.label.as_str());
                        ui.label(
                            RichText::new(state::truncated_key(&entry.public_key))
                                .font(text_mono())
                                .color(palette.text_muted),
                        );
                    });
                }
            });
    }

    form_row(ui, "Other key", |ui| {
        let response = ui.add(
            TextEdit::singleline(&mut picker.free_text)
                .hint_text("age1...")
                .font(text_mono())
                .desired_width(f32::INFINITY),
        );
        if response.changed() {
            picker.validate_free_text();
        }
    });
    if let Some(error) = &picker.free_error {
        ui.colored_label(palette.error, error);
    }
}

/// Renders the shared identity row: the remembered path (or a muted "none
/// chosen") plus the native file picker. A chosen path marks the config
/// dirty so it is remembered.
pub fn identity_row(ui: &mut Ui, identity_path: &mut Option<PathBuf>, config_dirty: &mut bool) {
    let palette = Palette::default();
    form_row(ui, "Identity", |ui| {
        match identity_path.as_ref() {
            Some(path) => {
                ui.label(RichText::new(path.display().to_string()).font(text_mono()));
            }
            None => {
                ui.label(RichText::new("none chosen").color(palette.text_muted));
            }
        }
        if ghost_button(ui, "Choose...").clicked() {
            // Blocking is correct here: the sync dialog is a native modal
            // and this runs on the main thread.
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Choose identity file")
                .pick_file()
            {
                *identity_path = Some(path);
                *config_dirty = true;
            }
        }
    });
}
