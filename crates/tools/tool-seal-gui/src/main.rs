mod app;
mod cli_utils;
mod config;
mod state;
mod views;
mod worker;

use crate::app::SealApp;
use common_cli::tool_exit_helpers::{exit_error, exit_success};
use egui::ViewportBuilder;
use tracing::error;

/// Desktop GUI shell for the seal tool family.
///
/// Boots logging (file sink on by default), opens the themed window, and
/// maps the eframe outcome onto the house exit codes: 0 on a clean close, 1
/// when the window or the app cannot be created.
fn main() {
    cli_utils::initialize();

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("seal")
            .with_app_id("seal-gui")
            .with_inner_size(egui::vec2(980.0, 720.0))
            .with_min_inner_size(egui::vec2(760.0, 520.0)),
        ..Default::default()
    };

    let outcome = eframe::run_native(
        "seal",
        options,
        Box::new(|cc| {
            common_gui::theme::apply_theme(&cc.egui_ctx, &common_gui::tokens::Palette::default());
            Ok(Box::new(SealApp::new(cc)?))
        }),
    );

    match outcome {
        Ok(()) => exit_success(),
        Err(e) => {
            error!(error = %e, "the window could not be created");
            exit_error();
        }
    }
}
