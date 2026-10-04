use crate::state;
use crate::state::AppState;
use crate::views;
use crate::worker::{JobRequest, WorkerHandle};
use anyhow::Context as _;
use common_gui::tokens::SPACE_XL;
use common_gui::widgets;
use common_gui::widgets::BadgeKind;
use common_utils::file_system::get_app_sub_folder;
use egui::{CentralPanel, Id, Layout, Panel};

/// Upper bound on worker events applied per frame, so a pathological burst
/// cannot stretch one frame.
const EVENT_BUDGET_PER_FRAME: usize = 64;

/// The three feature tabs of the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Text,
    Files,
    Keys,
}

/// The eframe application: owns the UI state and the worker bridge, renders
/// the chrome (toolbar and status bar), and routes the central area to the
/// active tab's view.
pub struct SealApp {
    worker: WorkerHandle,
    state: AppState,
    log_folder: String,
}

impl SealApp {
    /// # Errors
    /// Fails when the background worker thread cannot be spawned.
    pub fn new(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        let worker = WorkerHandle::spawn(cc.egui_ctx.clone())
            .context("spawning the background worker thread")?;

        Ok(Self {
            worker,
            state: AppState::default(),
            log_folder: get_app_sub_folder(env!("CARGO_PKG_NAME"), "logs")
                .display()
                .to_string(),
        })
    }

    fn submit_probe(&mut self) {
        if self.worker.submit(JobRequest::Probe) {
            // The Started event confirms it; set optimistically so the
            // action greys out this frame, not the next.
            self.state.busy = true;
        } else {
            state::on_worker_gone(&mut self.state);
        }
    }

    fn toolbar_contents(&mut self, ui: &mut egui::Ui) {
        ui.heading("seal");
        ui.small(env!("CARGO_PKG_VERSION"));
        for (tab, label) in [
            (Tab::Text, "Text"),
            (Tab::Files, "Files"),
            (Tab::Keys, "Keys"),
        ] {
            ui.selectable_value(&mut self.state.active_tab, tab, label);
        }
    }

    fn status_bar_contents(&mut self, ui: &mut egui::Ui) {
        if self.state.worker_gone {
            widgets::status_badge(ui, BadgeKind::Err, "worker stopped; restart the app");
        } else if self.state.busy {
            ui.spinner();
            match self.state.progress {
                Some((done, total)) => ui.small(format!("working... {done}/{total}")),
                None => ui.small("working..."),
            };
        }

        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            ui.small("logs").on_hover_text(self.log_folder.as_str());
            let can_probe = !self.state.busy && !self.state.worker_gone;
            let clicked = ui
                .add_enabled_ui(can_probe, |ui| {
                    widgets::ghost_button(ui, "Send a test toast")
                })
                .inner
                .clicked();
            if clicked {
                self.submit_probe();
            }
        });
    }
}

impl eframe::App for SealApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let (events, disconnected) = self.worker.drain(EVENT_BUDGET_PER_FRAME);
        for event in events {
            state::apply_event(&mut self.state, event);
        }
        if disconnected {
            state::on_worker_gone(&mut self.state);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        Panel::top(Id::new("toolbar")).show(ui, |ui| {
            widgets::toolbar(ui, |ui| self.toolbar_contents(ui));
        });

        Panel::bottom(Id::new("status")).show(ui, |ui| {
            ui.horizontal(|ui| self.status_bar_contents(ui));
        });

        // The central panel takes whatever space the fixed panels left, so
        // it must come after every other panel.
        CentralPanel::default().show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(SPACE_XL)
                .show(ui, |ui| match self.state.active_tab {
                    Tab::Text => views::text_tab::show(ui, &mut self.state),
                    Tab::Files => views::files_tab::show(ui, &mut self.state),
                    Tab::Keys => views::keys_tab::show(ui, &mut self.state),
                });
        });

        widgets::show_toasts(ui.ctx(), &mut self.state.toasts);
    }

    /// App state is the single source of truth; nothing egui memorized may
    /// survive a restart.
    fn persist_egui_memory(&self) -> bool {
        false
    }
}
