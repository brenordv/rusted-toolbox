use crate::config::{ConfigStore, FlushOutcome, SealConfig};
use crate::state::{self, AddOutcome, AppState, FilesTabState, PathKind};
use crate::views::{self, ViewIntent};
use crate::worker::{JobKind, JobRequest, WorkerHandle};
use anyhow::Context as _;
use common_gui::tokens::SPACE_XL;
use common_gui::widgets;
use common_gui::widgets::{BadgeKind, Toast, ToastKind};
use common_utils::file_system::get_app_sub_folder;
use egui::{CentralPanel, Id, Layout, Panel};
use std::path::{Path, PathBuf};

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

/// The eframe application: owns the UI state, the worker bridge, and the
/// config store; renders the chrome (toolbar and status bar) and routes the
/// central area to the active tab's view. Views hand back intents; this is
/// the one place jobs are submitted and the filesystem is touched.
pub struct SealApp {
    worker: WorkerHandle,
    state: AppState,
    config: ConfigStore,
    /// The previously rendered tab, so a switch can flush the config.
    last_tab: Tab,
    log_folder: String,
}

impl SealApp {
    /// # Errors
    /// Fails when the background worker thread cannot be spawned.
    pub fn new(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        let worker = WorkerHandle::spawn(cc.egui_ctx.clone())
            .context("spawning the background worker thread")?;

        let boot = ConfigStore::boot(|name| std::env::var_os(name));
        let state = AppState {
            book: boot.config.recipients,
            identity_path: boot.config.last_identity_file,
            files: FilesTabState {
                out_dir: boot.config.last_output_folder,
                overwrite: boot.config.overwrite_existing,
                ..FilesTabState::default()
            },
            status_note: boot.note.map(str::to_string),
            ..AppState::default()
        };

        Ok(Self {
            worker,
            state,
            config: boot.store,
            last_tab: Tab::Text,
            log_folder: get_app_sub_folder(env!("CARGO_PKG_NAME"), "logs")
                .display()
                .to_string(),
        })
    }

    fn submit(&mut self, job: JobRequest) {
        let kind = job.kind();
        if kind == JobKind::Files {
            self.state.files.begin_run();
        }
        if self.worker.submit(job) {
            // The Started event confirms it; set optimistically so actions
            // grey out this frame, not the next.
            self.state.busy = true;
            self.state.active_job = Some(kind);
        } else {
            state::on_worker_gone(&mut self.state);
        }
    }

    fn handle_intent(&mut self, intent: ViewIntent) {
        match intent {
            ViewIntent::Submit(job) => self.submit(job),
            ViewIntent::AddFiles(paths) => {
                for path in paths {
                    self.intake_path(path);
                }
            }
        }
    }

    /// Classifies one picked or dropped path and hands it to the pure intake
    /// rules. The filesystem probe lives here so the state stays testable.
    fn intake_path(&mut self, path: PathBuf) {
        let kind = match std::fs::metadata(&path) {
            Ok(meta) if meta.is_dir() => PathKind::Directory,
            Ok(meta) => PathKind::File { size: meta.len() },
            // Let the worker report the real failure at run time.
            Err(_) => PathKind::File { size: 0 },
        };
        if self.state.files.add_path(path, kind) == AddOutcome::DirectoryRejected {
            self.state.toasts.push(Toast::new(
                ToastKind::Warn,
                state::MSG_DIRECTORY_REJECTED,
                state::TOAST_TTL,
            ));
        }
    }

    /// Writes config-backed state when something marked it dirty. Only a
    /// successful save clears the flag, so exit retries after a refusal or a
    /// failure (both already reported once).
    fn flush_config_if_dirty(&mut self) {
        if !self.state.config_dirty {
            return;
        }
        let snapshot = SealConfig {
            recipients: self.state.book.clone(),
            last_identity_file: self.state.identity_path.clone(),
            last_output_folder: self.state.files.out_dir.clone(),
            overwrite_existing: self.state.files.overwrite,
            ..SealConfig::default()
        };
        match self.config.flush(&snapshot) {
            FlushOutcome::Saved => self.state.config_dirty = false,
            FlushOutcome::FailedFirst(error) => {
                self.state.toasts.push(Toast::new(
                    ToastKind::Err,
                    format!("settings could not be saved: {error}"),
                    state::TOAST_TTL,
                ));
            }
            FlushOutcome::Refused | FlushOutcome::NoLocation | FlushOutcome::FailedAgain => {}
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
        if let Some(note) = &self.state.status_note {
            widgets::status_badge(ui, BadgeKind::Warn, note);
        }

        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            ui.small("logs").on_hover_text(self.log_folder.as_str());
        });
    }
}

impl eframe::App for SealApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let (events, disconnected) = self.worker.drain(EVENT_BUDGET_PER_FRAME);
        for event in events {
            state::apply_event(&mut self.state, event);
        }
        if disconnected {
            state::on_worker_gone(&mut self.state);
        }

        // Drop intake runs every frame: dropped_files is populated only on
        // the frame the drop lands. The clones are cheap Arc copies.
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for handle in dropped {
            self.intake_path(handle.path().to_path_buf());
        }
        self.state.files.drop_hover = ctx.input(|i| !i.raw.hovered_files.is_empty());

        // Output-existence probes run here, outside the render pass, only
        // when something changed the computed outputs.
        if self.state.files.exists_refresh_pending {
            self.state.files.exists_refresh_pending = false;
            state::mark_existing_outputs(&mut self.state.files, |path: &Path| path.exists());
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
        let intent = CentralPanel::default()
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(SPACE_XL)
                    .show(ui, |ui| match self.state.active_tab {
                        Tab::Text => views::text_tab::show(ui, &mut self.state),
                        Tab::Files => views::files_tab::show(ui, &mut self.state),
                        Tab::Keys => views::keys_tab::show(ui, &mut self.state),
                    })
                    .inner
            })
            .inner;

        widgets::show_toasts(ui.ctx(), &mut self.state.toasts);

        if let Some(intent) = intent {
            self.handle_intent(intent);
        }

        // Tab switches debounce config writes without a timer.
        if self.state.active_tab != self.last_tab {
            self.last_tab = self.state.active_tab;
            self.flush_config_if_dirty();
        }
    }

    fn on_exit(&mut self) {
        self.flush_config_if_dirty();
    }

    /// App state is the single source of truth; nothing egui memorized may
    /// survive a restart.
    fn persist_egui_memory(&self) -> bool {
        false
    }
}
