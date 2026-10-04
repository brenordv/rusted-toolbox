use egui::{CentralPanel, Id, Panel};
use crate::state::AppState;
use crate::views;
use crate::worker::WorkerHandle;

#[derive(Clone, Copy, PartialEq)]
pub enum Tab { Text, Files, Keys }

pub struct SealApp {
    worker: WorkerHandle,
    state: AppState,
}

impl SealApp {
    pub(crate) fn on_worker_gone(&self) {
        todo!()
    }

    pub(crate) fn apply_event(&self, p0: _) {
        todo!()
    }
}


impl eframe::App for SealApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let (events, disconnected) = self.worker.drain(64);
        for ev in events { self.apply_event(ev); }     // busy flag, toasts, error!() on Failed
        if disconnected { self.on_worker_gone(); }     // error!() + toast + status line
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        Panel::top(Id::new("toolbar")).show(ui, |ui| {
            common_gui::widgets::toolbar(ui, |ui| {
                ui.heading("seal");
                ui.small(env!("CARGO_PKG_VERSION"));
                for (tab, label) in [(Tab::Text, "Text"), (Tab::Files, "Files"), (Tab::Keys, "Keys")] {
                    ui.selectable_value(&mut self.state.active_tab, tab, label);
                }
            });
        });

        Panel::bottom(Id::new("status")).show(ui, |ui| {
            ui.horizontal(|ui| {
                if self.state.busy { ui.spinner(); ui.small("working..."); }
                // right side: log-path tooltip hook
            });
        });

        CentralPanel::default().show(ui, |ui| {        // always last
            // SPACE_XL content inset via a Frame or margins
            match self.state.active_tab {
                Tab::Text  => views::text_tab::show(ui, &mut self.state),
                Tab::Files => views::files_tab::show(ui, &mut self.state),
                Tab::Keys  => views::keys_tab::show(ui, &mut self.state),
            }
        });

        common_gui::widgets::show_toasts(ui.ctx(), &mut self.state.toasts);
    }

    fn persist_egui_memory(&self) -> bool { false }    // config.json is the one store (phase 4)
}