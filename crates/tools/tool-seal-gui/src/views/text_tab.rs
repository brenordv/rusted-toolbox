use crate::state::{self, AppState};
use crate::views::{ViewIntent, picker};
use common_gui::tokens::text_mono;
use common_gui::widgets::{self, Toast, ToastKind};
use egui::{TextEdit, Ui};
use shared_crypto::Direction;

/// The Text tab: paste text and seal it to recipients as armor, or unseal
/// armored text back. All crypto happens on the worker thread.
pub fn show(ui: &mut Ui, state: &mut AppState) -> Option<ViewIntent> {
    let mut intent = None;

    widgets::section(ui, "Input", |ui| {
        let before = state.text.direction;
        // Locked while a job runs: a mid-run flip would clear the output box
        // that the running job is about to fill.
        ui.add_enabled_ui(!state.busy, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.text.direction, Direction::Encrypt, "Seal");
                ui.selectable_value(&mut state.text.direction, Direction::Decrypt, "Unseal");
            });
        });
        if state.text.direction != before {
            // Output from the other direction is stale at best.
            state.text.output.clear();
        }
        ui.add(
            TextEdit::multiline(&mut state.text.input)
                .font(text_mono())
                .desired_rows(8)
                .desired_width(f32::INFINITY)
                .hint_text(match state.text.direction {
                    Direction::Encrypt => "text to seal",
                    Direction::Decrypt => "paste armored text (-----BEGIN AGE ENCRYPTED FILE-----)",
                }),
        );
    });

    match state.text.direction {
        Direction::Encrypt => widgets::section(ui, "Recipients", |ui| {
            picker::recipient_picker(ui, &mut state.text.picker, &state.book, "text");
        }),
        Direction::Decrypt => widgets::section(ui, "Identity", |ui| {
            picker::identity_row(ui, &mut state.identity_path, &mut state.config_dirty);
        }),
    }

    widgets::section(ui, "Output", |ui| {
        // An immutable &str buffer keeps the box selectable for manual
        // partial copying while making every edit a no-op;
        // `.interactive(false)` would kill selection along with editing.
        ui.add(
            TextEdit::multiline(&mut state.text.output.as_str())
                .font(text_mono())
                .desired_rows(8)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal(|ui| {
            let job = state::text_job(state);
            let label = match state.text.direction {
                Direction::Encrypt => "Seal",
                Direction::Decrypt => "Unseal",
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

            let copy = ui
                .add_enabled_ui(!state.text.output.is_empty(), |ui| {
                    widgets::ghost_button(ui, "Copy")
                })
                .inner;
            if copy.clicked() {
                ui.ctx().copy_text(state.text.output.clone());
                state
                    .toasts
                    .push(Toast::new(ToastKind::Ok, "output copied", state::TOAST_TTL));
            }
        });
    });

    intent
}
