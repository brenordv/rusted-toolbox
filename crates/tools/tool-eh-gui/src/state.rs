//! The app state and its transition function.
//!
//! `apply` is pure: it changes the state and returns the effects the runtime glue carries out
//! (spawning work, quitting, reopening the window). It never touches a `Tx` or does I/O, so every
//! transition is testable without a window.

mod ending;

pub use ending::{Failure, FailureStage};

use crate::bridge::{EngineDone, EngineSession, UiDelivery};
use crate::design::Ds;
use crate::design::tokens::{SIDEBAR_MAX, SIDEBAR_MIN, SIDEBAR_WIDTH};
use crate::form::{ConnectForm, PendingSave};
use crate::services::{ConnectRequest, SaveRequest, Services};
use chrono::{DateTime, TimeDelta, Utc};
use common_cli::app_logger::FileLogging;
use ending::{log_summary, session_failures};
use rune_kit::PaneState;
use shared_eventhub_core::engine::{EngineEvent, RunEnd, RunSummary};
use shared_eventhub_persistence::paths::DataPaths;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// How long a closed window waits for a session's at-end pass before it reopens with a
/// "Still finishing" notice.
pub const STILL_FINISHING_AFTER: TimeDelta = TimeDelta::seconds(30);

/// The two sections of the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Read,
    Export,
}

impl Section {
    pub const ALL: [Section; 2] = [Self::Read, Self::Export];
    pub const LABELS: [&'static str; 2] = ["Read", "Export"];

    pub fn index(self) -> usize {
        match self {
            Self::Read => 0,
            Self::Export => 1,
        }
    }
}

/// Where the capture connection is. No state is final: the app can always reconnect.
#[derive(Debug, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    /// `session` is `None` while the store, secrets, and sink are prepared, then the session
    /// connecting. `fatal` keeps a fatal engine error for the coming `EngineDone`.
    Connecting {
        attempt: u64,
        session: Option<EngineSession>,
        fatal: Option<String>,
    },
    Connected {
        session: EngineSession,
        partitions: Vec<String>,
    },
    /// A stop was sent; the session ends once its at-end pass finishes.
    Stopping {
        session: EngineSession,
    },
    Failed(Failure),
}

impl ConnectionState {
    /// The running session, in every state that holds one.
    pub fn session(&self) -> Option<&EngineSession> {
        match self {
            Self::Connecting { session, .. } => session.as_ref(),
            Self::Connected { session, .. } | Self::Stopping { session } => Some(session),
            Self::Disconnected | Self::Failed(_) => None,
        }
    }
}

/// How a toast looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Danger,
}

/// The most toasts shown at once; older ones leave first.
const MAX_TOASTS: usize = 4;
/// Ticks (seconds) an info toast stays; danger toasts stay until closed.
const INFO_TOAST_TICKS: u8 = 6;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub text: String,
    ticks_left: Option<u8>,
}

/// The result of the last Save config, shown as a caption in the sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveStatus {
    Idle,
    /// A save runs; what it writes for each secret updates the form when it lands.
    Saving(PendingSave),
    Saved,
    Failed(String),
}

impl SaveStatus {
    pub fn is_saving(&self) -> bool {
        match self {
            Self::Saving(_) => true,
            Self::Idle | Self::Saved | Self::Failed(_) => false,
        }
    }
}

/// Something that happened: a user action, a background result, or the clock.
#[derive(Debug)]
pub enum Input {
    SectionPicked(Section),
    OpenSheet,
    CloseSheet,
    ConnectPressed,
    DisconnectPressed,
    SavePressed,
    ConfigSaved(Result<(), String>),
    DataFolderPicked(PathBuf),
    DismissToast(u64),
    WindowClosed(DateTime<Utc>),
    ForceQuit,
    Tick(DateTime<Utc>),
    SessionReady {
        attempt: u64,
        result: Result<EngineSession, String>,
    },
    Delivery(UiDelivery),
    EngineDone(EngineDone),
}

/// Work `apply` asks the runtime glue to do.
#[derive(Debug)]
pub enum Effect {
    /// Open the store, resolve the target, build the sink, and start the session; the result
    /// comes back as [`Input::SessionReady`].
    PrepareSession {
        attempt: u64,
        request: ConnectRequest,
    },
    /// Send Stop to the current session.
    StopSession,
    /// Send Stop to a session nothing holds any more.
    StopOrphan(EngineSession),
    SaveConfig(SaveRequest),
    /// Point the store at another data folder and release the old one's lock.
    SwitchDataFolder(DataPaths),
    ReopenWindow,
    /// Close the store and end the process.
    Quit,
}

/// What a transition did: the effects to run, and whether anything the view reads changed.
#[derive(Debug, Default)]
pub struct Outcome {
    pub effects: Vec<Effect>,
    pub changed: bool,
}

/// The whole UI state.
pub struct AppState {
    pub services: Option<Box<dyn Services>>,
    /// The two kits, built once.
    pub ds: Ds,
    pub sidebar: PaneState,
    /// Whether the connection pill's popover shows.
    pub pill_open: bool,
    pub section: Section,
    pub connection: ConnectionState,
    pub form: ConnectForm,
    pub sheet_open: bool,
    /// Set by the first Connect or Save with form issues, so an untouched form shows none.
    pub show_issues: bool,
    pub save_status: SaveStatus,
    /// What happened to `config.json` at launch, for the sheet's caption.
    pub config_caption: Option<String>,
    pub file_logging: FileLogging,
    pub toasts: Vec<Toast>,
    /// Shown after the window reopened because the at-end pass ran past
    /// [`STILL_FINISHING_AFTER`]; offers Force quit.
    pub still_finishing: bool,
    window_closed: bool,
    closed_at: Option<DateTime<Utc>>,
    next_attempt: u64,
    next_toast: u64,
    /// Set once `Quit` was emitted; the app is ending and nothing asks again.
    quit_requested: bool,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("section", &self.section)
            .field("connection", &self.connection)
            .field("form", &self.form)
            .field("window_closed", &self.window_closed)
            .finish_non_exhaustive()
    }
}

impl AppState {
    /// A state with no services, for `AppSpec`'s plain constructor; `App::init` replaces it
    /// before the first frame.
    pub fn detached() -> Self {
        Self::new(None, ConnectForm::default(), None, FileLogging::Off)
    }

    pub fn new(
        services: Option<Box<dyn Services>>,
        form: ConnectForm,
        config_caption: Option<String>,
        file_logging: FileLogging,
    ) -> Self {
        Self {
            services,
            ds: Ds::new(),
            sidebar: PaneState::new(SIDEBAR_WIDTH).range(SIDEBAR_MIN, SIDEBAR_MAX),
            pill_open: false,
            section: Section::Read,
            connection: ConnectionState::Disconnected,
            form,
            sheet_open: false,
            show_issues: false,
            save_status: SaveStatus::Idle,
            config_caption,
            file_logging,
            toasts: Vec::new(),
            still_finishing: false,
            window_closed: false,
            closed_at: None,
            next_attempt: 1,
            next_toast: 1,
            quit_requested: false,
        }
    }

    /// Shows a toast, dropping the oldest past [`MAX_TOASTS`].
    pub fn toast(&mut self, kind: ToastKind, text: String) {
        let ticks_left = match kind {
            ToastKind::Info => Some(INFO_TOAST_TICKS),
            ToastKind::Danger => None,
        };
        self.toasts.push(Toast {
            id: self.next_toast,
            kind,
            text,
            ticks_left,
        });
        self.next_toast += 1;
        if self.toasts.len() > MAX_TOASTS {
            self.toasts.remove(0);
        }
    }

    /// Applies `input` and returns the effects to run.
    pub fn apply(&mut self, input: Input) -> Outcome {
        let mut out = Outcome {
            effects: Vec::new(),
            changed: true,
        };
        match input {
            Input::SectionPicked(section) => self.section = section,
            Input::OpenSheet => self.sheet_open = true,
            Input::CloseSheet => {
                self.sheet_open = false;
                self.form.hide_secrets();
            }
            Input::ConnectPressed => self.connect_pressed(&mut out),
            Input::DisconnectPressed => self.disconnect_pressed(&mut out),
            Input::SavePressed => self.save_pressed(&mut out),
            Input::ConfigSaved(result) => self.config_saved(result),
            Input::DataFolderPicked(folder) => self.data_folder_picked(folder, &mut out),
            Input::DismissToast(id) => self.toasts.retain(|toast| toast.id != id),
            Input::WindowClosed(at) => self.window_closed(at, &mut out),
            Input::ForceQuit => self.force_quit(&mut out),
            Input::Tick(now) => self.tick(now, &mut out),
            Input::SessionReady { attempt, result } => {
                self.session_ready(attempt, result, &mut out);
            }
            Input::Delivery(delivery) => self.delivery(delivery, &mut out),
            Input::EngineDone(done) => self.engine_done(done, &mut out),
        }
        self.quit_rule(&mut out);
        out
    }

    fn connect_pressed(&mut self, out: &mut Outcome) {
        match &self.connection {
            ConnectionState::Disconnected | ConnectionState::Failed(_) => {}
            ConnectionState::Connecting { .. }
            | ConnectionState::Connected { .. }
            | ConnectionState::Stopping { .. } => {
                debug!("Connect ignored: a session is already starting or running");
                out.changed = false;
                return;
            }
        }
        if self.save_status.is_saving() {
            debug!("Connect ignored: a save holds the secrets file");
            out.changed = false;
            return;
        }
        if !self.form.issues().is_empty() {
            debug!("Connect ignored: the form has issues");
            self.show_issues = true;
            return;
        }

        let attempt = self.next_attempt;
        self.next_attempt += 1;
        let (config, given) = self.form.to_config();
        let paths = self.form.data_paths();
        info!(attempt, "connecting");
        self.connection = ConnectionState::Connecting {
            attempt,
            session: None,
            fatal: None,
        };
        self.sheet_open = false;
        self.form.hide_secrets();
        out.effects.push(Effect::PrepareSession {
            attempt,
            request: ConnectRequest {
                config,
                given,
                paths,
            },
        });
    }

    fn disconnect_pressed(&mut self, out: &mut Outcome) {
        match std::mem::take(&mut self.connection) {
            ConnectionState::Connecting { session: None, .. } => {
                info!("connect cancelled while preparing");
            }
            ConnectionState::Connecting {
                session: Some(session),
                ..
            }
            | ConnectionState::Connected { session, .. } => {
                info!(session = session.id(), "stopping the session");
                self.connection = ConnectionState::Stopping { session };
                out.effects.push(Effect::StopSession);
            }
            other @ (ConnectionState::Disconnected
            | ConnectionState::Failed(_)
            | ConnectionState::Stopping { .. }) => {
                self.connection = other;
                out.changed = false;
            }
        }
    }

    fn save_pressed(&mut self, out: &mut Outcome) {
        if self.save_status.is_saving() {
            out.changed = false;
            return;
        }
        if !self.form.issues().is_empty() {
            self.show_issues = true;
            return;
        }
        let (config, given) = self.form.to_config();
        self.save_status = SaveStatus::Saving(self.form.pending_save());
        out.effects.push(Effect::SaveConfig(SaveRequest {
            config,
            given,
            paths: self.form.data_paths(),
        }));
    }

    fn config_saved(&mut self, result: Result<(), String>) {
        let pending = match std::mem::replace(&mut self.save_status, SaveStatus::Idle) {
            SaveStatus::Saving(pending) => Some(pending),
            SaveStatus::Idle | SaveStatus::Saved | SaveStatus::Failed(_) => None,
        };
        match result {
            Ok(()) => {
                match &pending {
                    Some(pending) => self.form.settle_after_save(pending),
                    None => debug!("a save finished that this state did not start"),
                }
                self.save_status = SaveStatus::Saved;
            }
            Err(text) => {
                warn!(error = %text, "saving config.json failed");
                self.toast(
                    ToastKind::Danger,
                    format!("Could not save config.json: {text}"),
                );
                self.save_status = SaveStatus::Failed(text);
            }
        }
    }

    /// Accepted only while no session can be using the old folder.
    fn data_folder_picked(&mut self, folder: PathBuf, out: &mut Outcome) {
        match &self.connection {
            ConnectionState::Disconnected | ConnectionState::Failed(_) => {}
            ConnectionState::Connecting { .. }
            | ConnectionState::Connected { .. }
            | ConnectionState::Stopping { .. } => {
                debug!("data folder change refused: a session is starting or running");
                out.changed = false;
                return;
            }
        }
        if self.save_status.is_saving() {
            debug!("data folder change refused: a save to the current folder is running");
            out.changed = false;
            return;
        }
        self.form.data_folder = folder.display().to_string();
        self.form.forget_stored_secrets();
        let paths = self.form.data_paths();
        info!(folder = %paths.base().display(), "data folder changed");
        out.effects.push(Effect::SwitchDataFolder(paths));
    }

    fn window_closed(&mut self, at: DateTime<Utc>, out: &mut Outcome) {
        self.window_closed = true;
        self.closed_at = Some(at);
        self.sheet_open = false;
        self.form.hide_secrets();
        match std::mem::take(&mut self.connection) {
            ConnectionState::Connecting { session: None, .. } => {
                info!("window closed while preparing; the attempt is abandoned");
            }
            ConnectionState::Connecting {
                session: Some(session),
                ..
            }
            | ConnectionState::Connected { session, .. }
            | ConnectionState::Stopping { session } => {
                info!("window closed; finishing session before exit");
                self.connection = ConnectionState::Stopping { session };
                out.effects.push(Effect::StopSession);
            }
            other @ (ConnectionState::Disconnected | ConnectionState::Failed(_)) => {
                self.connection = other;
            }
        }
    }

    fn tick(&mut self, now: DateTime<Utc>, out: &mut Outcome) {
        let before = self.toasts.len();
        for toast in &mut self.toasts {
            if let Some(ticks) = &mut toast.ticks_left {
                *ticks = ticks.saturating_sub(1);
            }
        }
        self.toasts.retain(|toast| toast.ticks_left != Some(0));
        out.changed = self.toasts.len() != before;

        let overdue = matches!(self.connection, ConnectionState::Stopping { .. })
            && self.window_closed
            && self
                .closed_at
                .is_some_and(|closed| now - closed > STILL_FINISHING_AFTER);
        if overdue {
            info!("the session is still finishing; reopening the window");
            self.reopen(out);
            self.still_finishing = true;
            // The notice and its Force quit live in the Read section.
            self.section = Section::Read;
            out.changed = true;
        }
    }

    /// Quits without waiting for the at-end pass; only offered while a closed window's session
    /// is still finishing.
    fn force_quit(&mut self, out: &mut Outcome) {
        let finishing =
            matches!(self.connection, ConnectionState::Stopping { .. }) && self.still_finishing;
        if !finishing {
            debug!("Force quit ignored: no session is still finishing");
            out.changed = false;
            return;
        }
        warn!("force quit: the session's at-end pass was abandoned");
        self.request_quit(out);
    }

    fn session_ready(
        &mut self,
        attempt: u64,
        result: Result<EngineSession, String>,
        out: &mut Outcome,
    ) {
        let pending = matches!(
            &self.connection,
            ConnectionState::Connecting { attempt: current, session: None, .. } if *current == attempt
        );
        match (pending, result) {
            (true, Ok(session)) => {
                info!(attempt, "session started");
                self.form.clear_secrets();
                self.connection = ConnectionState::Connecting {
                    attempt,
                    session: Some(session),
                    fatal: None,
                };
            }
            (true, Err(text)) => {
                warn!(attempt, error = %text, "the session could not start");
                self.toast(ToastKind::Danger, format!("Could not connect: {text}"));
                self.connection = ConnectionState::Failed(Failure::starting(text));
            }
            (false, Ok(session)) => {
                debug!(
                    attempt,
                    "a session started after its attempt was abandoned; stopping it"
                );
                out.effects.push(Effect::StopOrphan(session));
                out.changed = false;
            }
            (false, Err(text)) => {
                debug!(attempt, error = %text, "an abandoned attempt failed");
                out.changed = false;
            }
        }
    }

    fn delivery(&mut self, delivery: UiDelivery, out: &mut Outcome) {
        out.changed = false;
        if self.connection.session().map(EngineSession::id) != Some(delivery.session) {
            debug!(
                session = delivery.session,
                "ignored a delivery from another session"
            );
            return;
        }
        for event in delivery.events {
            match event {
                EngineEvent::Started { partitions } => self.started(partitions, out),
                EngineEvent::Error {
                    partition_id,
                    error,
                    fatal: true,
                } => {
                    debug!(partition = ?partition_id, "the engine reported a fatal error");
                    if let ConnectionState::Connecting { fatal, .. } = &mut self.connection {
                        *fatal = Some(error);
                    }
                }
                EngineEvent::Error {
                    partition_id,
                    error,
                    fatal: false,
                } => {
                    warn!(partition = ?partition_id, error = %error, "a partition reported an error");
                }
                EngineEvent::Message(_)
                | EngineEvent::Skipped { .. }
                | EngineEvent::PartitionEnded { .. } => {}
            }
        }
    }

    fn started(&mut self, partitions: Vec<String>, out: &mut Outcome) {
        match std::mem::take(&mut self.connection) {
            ConnectionState::Connecting {
                session: Some(session),
                ..
            } => {
                info!(
                    session = session.id(),
                    partitions = partitions.len(),
                    "connected"
                );
                self.connection = ConnectionState::Connected {
                    session,
                    partitions,
                };
                out.changed = true;
            }
            other @ (ConnectionState::Disconnected
            | ConnectionState::Connecting { session: None, .. }
            | ConnectionState::Connected { .. }
            | ConnectionState::Stopping { .. }
            | ConnectionState::Failed(_)) => self.connection = other,
        }
    }

    fn engine_done(&mut self, done: EngineDone, out: &mut Outcome) {
        if self.connection.session().map(EngineSession::id) != Some(done.session) {
            debug!(session = done.session, "ignored the end of another session");
            out.changed = false;
            return;
        }
        let (fatal, stage) = match &mut self.connection {
            ConnectionState::Connecting { fatal, .. } => (fatal.take(), FailureStage::Starting),
            ConnectionState::Connected { .. }
            | ConnectionState::Stopping { .. }
            | ConnectionState::Disconnected
            | ConnectionState::Failed(_) => (None, FailureStage::Run),
        };
        if let Ok(summary) = &done.engine {
            log_summary(done.session, summary, &done.finish);
        }

        match session_failures(&done.engine, &done.finish, fatal) {
            None => {
                if let Ok(RunSummary {
                    end: RunEnd::Completed,
                    ..
                }) = &done.engine
                {
                    self.toast(
                        ToastKind::Info,
                        "Every partition reached the end of the time window".to_owned(),
                    );
                }
                self.connection = ConnectionState::Disconnected;
            }
            Some(text) => {
                warn!(session = done.session, error = %text, "the session failed");
                self.toast(ToastKind::Danger, text.clone());
                self.connection = ConnectionState::Failed(Failure { stage, text });
                if self.window_closed {
                    self.reopen(out);
                }
            }
        }
        self.still_finishing = false;
    }

    fn reopen(&mut self, out: &mut Outcome) {
        self.window_closed = false;
        self.closed_at = None;
        out.effects.push(Effect::ReopenWindow);
    }

    /// Quits once the window is closed and neither a session nor a save is left running. Asks
    /// for the quit once; the quit path then runs to the end on its own.
    fn quit_rule(&mut self, out: &mut Outcome) {
        if self.quit_requested
            || !self.window_closed
            || self.connection.session().is_some()
            || self.save_status.is_saving()
        {
            return;
        }
        info!("window closed and nothing runs; quitting");
        self.request_quit(out);
    }

    fn request_quit(&mut self, out: &mut Outcome) {
        self.quit_requested = true;
        out.effects.push(Effect::Quit);
    }

    /// Whether the window is closed (the app keeps running for a session's at-end pass).
    pub fn window_is_closed(&self) -> bool {
        self.window_closed
    }
}

#[cfg(test)]
mod tests;
