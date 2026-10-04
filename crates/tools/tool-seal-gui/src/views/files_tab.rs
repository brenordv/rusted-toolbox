use crate::state::AppState;
use common_gui::tokens::Palette;
use common_gui::widgets;
use egui::{RichText, Ui};

/// Placeholder body for the Files tab.
pub fn show(ui: &mut Ui, _state: &mut AppState) {
    widgets::section(ui, "Files", |ui| {
        ui.label(
            RichText::new(
                "Seal and open files here, drag-and-drop included. Coming in the next phase.",
            )
            .color(Palette::default().text_muted),
        );
    });
}
