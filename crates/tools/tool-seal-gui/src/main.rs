pub mod app;
pub mod worker;
pub mod state;
pub mod views;

fn main() {
    // common-cli boot first: args (no-verbose variant), AppLogger with file logging
    // ON by default for the GUI (a windowed app has no visible stderr), flags override.

    let options = eframe::NativeOptions {
        // Window geometry and app id ride on NativeOptions' ViewportBuilder:
        // initial 900x640, min 720x480, app id "seal-gui". Lift the exact with_*
        // builder names from the eframe root page at implementation (names
        // unverified here; the page is linked in section 7). Leave drag-and-drop
        // enabled: phase 4's Files tab needs it, and on Windows dropped_files is
        // documented to stay empty if it is disabled.
        ..Default::default()
    };

    eframe::run_native(
        "seal",
        options,
        Box::new(|cc| {
            common_gui::theme::apply_theme(&cc.egui_ctx, &common_gui::tokens::Palette::default());
            Ok(Box::new(SealApp::new(cc)))
        }),
    )
    // map the eframe result into the house exit helpers
}