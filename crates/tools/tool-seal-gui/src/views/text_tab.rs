use crate::state::AppState;
use common_gui::tokens::Palette;
use common_gui::widgets;
use egui::{RichText, Ui};

/// Placeholder body for the Text tab.
pub fn show(ui: &mut Ui, _state: &mut AppState) {
    widgets::section(ui, "Text", |ui| {
        ui.label(
            RichText::new("Seal and open text snippets here. Coming in the next phase.")
                .color(Palette::default().text_muted),
        );
    });
}
