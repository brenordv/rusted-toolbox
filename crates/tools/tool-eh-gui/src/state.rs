//! The app state and its transition function.
//!
//! `apply` is pure: it changes the state and returns the effects the runtime glue carries out
//! (spawning work, quitting, reopening the window). It never touches a `Tx` or does I/O, so every
//! transition is testable without a window.

mod details;
mod ending;
mod new_profile;
mod panes;
mod profiles;
mod send;
mod toasts;

pub use details::{ERROR_PREVIEW_CHARS, ErrorDetails, ErrorPreview, error_preview};
pub use ending::{Failure, FailureStage};
pub use new_profile::{
    NewProfileRow, ProfileDestination, ProfileFocus, RECALLED_FIELD, RowIssue, RowLine,
};
pub use panes::{Pane, PaneFit};
pub use profiles::{
    LAUNCH_FOLDER, NO_PROFILES, NO_WORKSPACE, ProfileList, ProfileNote, ProfileSwitch, Recalled,
    home_label, recalled,
};
pub use toasts::{INFO_TOAST_SECS, MAX_TOASTS, ToastBody, ToastInput};

use crate::archive::bulk::CancelReason;
use crate::archive::{ArchiveEffect, ArchiveInput, ArchiveState};
use crate::bridge::{EngineDone, EngineSession, UiDelivery};
use crate::design::Ds;
use crate::design::tokens::{SIDEBAR_MAX, SIDEBAR_MIN, SIDEBAR_WIDTH};
use crate::form::{ConnectForm, PendingSave};
use crate::read::inspector::InspectorInput;
use crate::read::{LogKind, ReadEffect, ReadInput, ReadState};
use crate::send::run::Names;
use crate::send::{SendEffect, SendInput, SendState};
use crate::services::{ConnectRequest, CreateRequest, SaveRequest, Services};
use chrono::{DateTime, TimeDelta, Utc};
use common_cli::app_logger::FileLogging;
use common_utils::file_system::get_current_dir;
use ending::{log_summary, session_failures};
use rune::TextEdit;
use rune_kit::PaneState;
use shared_eventhub_core::engine::{EngineEvent, RunEnd, RunSummary};
use shared_eventhub_persistence::config::{ConfigLoad, SensitiveValues, ToolConfig};
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::workspace::{
    ConfigHome, Profile, ProfileEntry, ProfileName, Workspace,
};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, info, warn};

/// How long a closed window waits for a session's at-end pass before it reopens with a
/// "Still finishing" notice.
pub const STILL_FINISHING_AFTER: TimeDelta = TimeDelta::seconds(30);
/// How long a send may take to stop after a cancel, and to close its producer.
pub const SEND_STOP_BOUND: Duration = Duration::from_secs(30);

/// What the connection sheet is open for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetPurpose {
    /// Connect to capture; the footer's primary action is Connect.
    Connect,
    /// Choose where the Send section sends; the footer's primary action is Done.
    SendTarget,
}

/// The three sections of the window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Section {
    #[default]
    Read,
    Export,
    Send,
}

impl Section {
    pub const ALL: [Section; 3] = [Self::Read, Self::Export, Self::Send];
    pub const LABELS: [&'static str; 3] = ["Read", "Export", "Send"];

    pub fn index(self) -> usize {
        match self {
            Self::Read => 0,
            Self::Export => 1,
            Self::Send => 2,
        }
    }

    /// The message list this section shows, when it shows one.
    pub fn list(self) -> Option<ListSection> {
        match self {
            Self::Read => Some(ListSection::Read),
            Self::Export => Some(ListSection::Export),
            Self::Send => None,
        }
    }
}

/// The sections that show a message list and an inspector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListSection {
    Read,
    Export,
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

    /// Whether a session is preparing, running, or finishing: the form's target is in use.
    pub fn is_active(&self) -> bool {
        match self {
            Self::Connecting { .. } | Self::Connected { .. } | Self::Stopping { .. } => true,
            Self::Disconnected | Self::Failed(_) => false,
        }
    }
}

/// How a toast looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Danger,
}

/// The result of the last save (Save config or Save profile), shown as a caption in the sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveStatus {
    Idle,
    /// A save runs to where `SaveTo` says.
    Saving(SaveTo),
    Saved,
    Failed(String),
}

/// Where a running save writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveTo {
    /// The home's `config.json`; what it writes for each secret updates the form when it lands.
    Home(PendingSave),
    /// A new profile folder named `name`, for create `request`.
    NewProfile { name: ProfileName, request: u64 },
}

impl SaveStatus {
    pub fn is_saving(&self) -> bool {
        match self {
            Self::Saving(_) => true,
            Self::Idle | Self::Saved | Self::Failed(_) => false,
        }
    }

    /// The name of the profile a running save creates; `None` for any other status.
    pub fn creating(&self) -> Option<&ProfileName> {
        match self {
            Self::Saving(SaveTo::NewProfile { name, .. }) => Some(name),
            Self::Saving(SaveTo::Home(_)) | Self::Idle | Self::Saved | Self::Failed(_) => None,
        }
    }
}

/// Something that happened: a user action, a background result, or the clock.
#[derive(Debug)]
pub enum Input {
    SectionPicked(Section),
    OpenSheet(SheetPurpose),
    CloseSheet,
    ConnectPressed,
    DisconnectPressed,
    SavePressed,
    ConfigSaved(Result<(), String>),
    DataFolderPicked(PathBuf),
    Toast(ToastInput),
    /// Open the error details modal on the whole of `text`; `toast` names the card it opened
    /// from, if any.
    ShowDetails {
        title: String,
        text: String,
        toast: Option<u64>,
    },
    CloseDetails,
    /// The details modal's text reached the clipboard.
    DetailsCopied(DateTime<Utc>),
    WindowClosed(DateTime<Utc>),
    ForceQuit,
    Tick(DateTime<Utc>),
    SessionReady {
        attempt: u64,
        result: Result<EngineSession, String>,
    },
    Delivery(UiDelivery),
    EngineDone(EngineDone),
    /// The system's appearance, read by the app itself (true: dark).
    AppearanceChanged(bool),
    /// A pick of where `config.json` comes from.
    ProfilePicked(ConfigHome),
    /// The answer to [`Effect::LoadProfile`] `request`.
    ProfileLoaded {
        request: u64,
        result: Result<ConfigLoad, String>,
    },
    /// The answer to [`Effect::ListProfiles`] `request`.
    ProfilesListed {
        request: u64,
        result: Result<Vec<ProfileEntry>, String>,
    },
    /// Save as profile… was pressed: the row with the profile name opens.
    NewProfileOpened,
    /// Cancel, or Escape in the profile name field.
    NewProfileCancelled,
    /// The profile name field changed.
    NewProfileNameEdited,
    /// Save profile (Save anyway while the row asks to confirm).
    CreateProfilePressed,
    /// Return in the profile name field.
    NewProfileNameSubmitted,
    /// The answer to [`Effect::CreateProfile`] `request`: the new profile, or why it was not
    /// saved.
    ProfileCreated {
        request: u64,
        result: Result<Profile, String>,
    },
    Read(ReadInput),
    Archive(ArchiveInput),
    Send(SendInput),
}

impl Input {
    /// `input` for the inspector of `section`.
    pub fn inspector(section: ListSection, input: InspectorInput) -> Self {
        match section {
            ListSection::Read => Self::Read(ReadInput::Inspector(input)),
            ListSection::Export => Self::Archive(ArchiveInput::Inspector(input)),
        }
    }
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
    /// List `workspace`'s profiles; the result comes back as [`Input::ProfilesListed`].
    ListProfiles {
        request: u64,
        workspace: Workspace,
    },
    /// Read `dir`'s `config.json`; the result comes back as [`Input::ProfileLoaded`].
    LoadProfile {
        request: u64,
        dir: PathBuf,
    },
    /// Save the form as a new profile; the result comes back as [`Input::ProfileCreated`].
    CreateProfile(CreateRequest),
    ReopenWindow,
    /// Close the store and end the process.
    Quit,
    Read(ReadEffect),
    Archive(ArchiveEffect),
    Send(SendEffect),
}

/// What a transition did: the effects to run, whether anything the view reads changed, and
/// how its commit moves. A commit no input caused lands without motion unless it asks for it.
#[derive(Debug, Default)]
pub struct Outcome {
    pub effects: Vec<Effect>,
    pub changed: bool,
    pub motion: Motion,
}

/// How a commit moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Motion {
    /// As the runtime decides: a commit no input caused lands without motion.
    #[default]
    Still,
    /// Animate, though no input caused it (digit rolls, the first-connect entrance).
    Animated,
    /// Land without motion, whatever else in the frame asks for it (rows moved onto the list).
    Instant,
}

/// The whole UI state.
pub struct AppState {
    pub services: Option<Box<dyn Services>>,
    /// The two kits, built once.
    pub ds: Ds,
    pub sidebar: PaneState,
    /// The window's width at its last layout report, in whole points; `None` before the first.
    /// Reports skip moves under half a point, so it can be a point off the window's width.
    pub window_width: Option<f32>,
    /// Whether the connection pill's popover shows.
    pub pill_open: bool,
    pub section: Section,
    pub connection: ConnectionState,
    pub form: ConnectForm,
    /// The open connection sheet's purpose; `None` while it is closed.
    pub sheet: Option<SheetPurpose>,
    /// Whether the sheet's profile picker lists its options.
    pub profile_picker_open: bool,
    /// Where `config.json` lives, the picker's list, and the pending load.
    profiles: profiles::Profiles,
    /// The name typed in the Save as profile row; emptied when the row opens and after a save.
    pub(crate) new_profile_name: TextEdit,
    /// Whether the row shows, and why its last Save profile could not start.
    new_profile: NewProfileRow,
    /// The row control focus moves to once the view shows it, until taken.
    pending_focus: Option<ProfileFocus>,
    /// Connect attempts abandoned while preparing, until each answers; they hold the data
    /// folder meanwhile.
    abandoned_attempts: Vec<u64>,
    /// Set by the first Connect or Save with form issues, so an untouched form shows none.
    pub show_issues: bool,
    pub save_status: SaveStatus,
    /// What happened to the home's `config.json` when it was last read, as caption text.
    pub config_caption: Option<String>,
    pub file_logging: FileLogging,
    toasts: toasts::Toasts,
    /// The error details modal; `None` while it is closed.
    details: Option<ErrorDetails>,
    /// Shown after the window reopened because the at-end pass ran past
    /// [`STILL_FINISHING_AFTER`]; offers Force quit.
    pub still_finishing: bool,
    pub read: ReadState,
    pub archive: ArchiveState,
    pub send: SendState,
    /// The system's appearance as the app read it; `None` leaves Rune's.
    pub system_dark: Option<bool>,
    window_closed: bool,
    closed_at: Option<DateTime<Utc>>,
    next_attempt: u64,
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

    /// A state on the process's current folder with no workspace;
    /// `set_launch` names the launch folder and the workspace.
    pub fn new(
        services: Option<Box<dyn Services>>,
        form: ConnectForm,
        config_caption: Option<String>,
        file_logging: FileLogging,
    ) -> Self {
        let mut send = SendState::default();
        send.refresh_destination(&form);
        Self {
            services,
            ds: Ds::new(),
            sidebar: PaneState::new(SIDEBAR_WIDTH).range(SIDEBAR_MIN, SIDEBAR_MAX),
            window_width: None,
            pill_open: false,
            section: Section::Read,
            connection: ConnectionState::Disconnected,
            form,
            sheet: None,
            profile_picker_open: false,
            profiles: profiles::Profiles::new(get_current_dir()),
            new_profile_name: TextEdit::new(""),
            new_profile: NewProfileRow::Closed,
            pending_focus: None,
            abandoned_attempts: Vec::new(),
            show_issues: false,
            save_status: SaveStatus::Idle,
            config_caption,
            file_logging,
            toasts: toasts::Toasts::default(),
            details: None,
            still_finishing: false,
            read: ReadState::default(),
            archive: ArchiveState::default(),
            send,
            system_dark: None,
            window_closed: false,
            closed_at: None,
            next_attempt: 1,
            quit_requested: false,
        }
    }

    /// Applies `input` and returns the effects to run.
    pub fn apply(&mut self, input: Input) -> Outcome {
        let mut out = Outcome {
            changed: true,
            ..Outcome::default()
        };
        let toasts_before = self.toast_mark();
        match input {
            Input::SectionPicked(section) => self.section = section,
            Input::OpenSheet(purpose) => self.open_sheet(purpose, &mut out),
            Input::CloseSheet => self.close_sheet(),
            Input::ConnectPressed => self.connect_pressed(&mut out),
            Input::DisconnectPressed => self.disconnect_pressed(&mut out),
            Input::SavePressed => self.save_pressed(&mut out),
            Input::ConfigSaved(result) => self.config_saved(result, &mut out),
            Input::DataFolderPicked(folder) => self.data_folder_picked(folder, &mut out),
            Input::Toast(input) => self.toast_input(input, &mut out),
            Input::ShowDetails { title, text, toast } => self.show_details(title, &text, toast),
            Input::CloseDetails => self.close_details(&mut out),
            Input::DetailsCopied(now) => self.details_copied(now, &mut out),
            Input::WindowClosed(at) => self.window_closed(at, &mut out),
            Input::ForceQuit => self.force_quit(&mut out),
            Input::Tick(now) => self.tick(now, &mut out),
            Input::SessionReady { attempt, result } => {
                self.session_ready(attempt, result, &mut out);
            }
            Input::Delivery(delivery) => self.delivery(delivery, &mut out),
            Input::EngineDone(done) => self.engine_done(done, &mut out),
            Input::AppearanceChanged(dark) => self.appearance_changed(dark, &mut out),
            Input::ProfilePicked(home) => self.profile_picked(home, &mut out),
            Input::ProfileLoaded { request, result } => {
                self.profile_loaded(request, result, &mut out);
            }
            Input::ProfilesListed { request, result } => {
                self.profiles_listed(request, result, &mut out);
            }
            Input::NewProfileOpened => self.new_profile_opened(&mut out),
            Input::NewProfileCancelled => self.new_profile_cancelled(&mut out),
            Input::NewProfileNameEdited => self.new_profile_name_edited(&mut out),
            Input::CreateProfilePressed => self.create_pressed(&mut out),
            Input::NewProfileNameSubmitted => self.new_profile_name_submitted(&mut out),
            Input::ProfileCreated { request, result } => {
                self.profile_created(request, result, &mut out);
            }
            Input::Read(input) => self.read_input(input, &mut out),
            Input::Archive(input) => self.archive_input(input, &mut out),
            Input::Send(input) => self.send_input(input, &mut out),
        }
        self.settle_toasts(toasts_before, &mut out);
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
        match self.sheet {
            None | Some(SheetPurpose::Connect) => {}
            Some(SheetPurpose::SendTarget) => {
                debug!("Connect ignored: the sheet is open to choose where to send");
                out.changed = false;
                return;
            }
        }
        if self.profile_loading() {
            debug!("Connect ignored: a profile is loading");
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
        let (config, given) = self.config_with_rules();
        let paths = self.data_paths();
        info!(attempt, "connecting");
        self.read.start_session(
            attempt,
            Duration::from_secs(config.capture.feedback_interval_secs),
        );
        self.connection = ConnectionState::Connecting {
            attempt,
            session: None,
            fatal: None,
        };
        self.close_sheet();
        out.effects.push(Effect::PrepareSession {
            attempt,
            request: ConnectRequest {
                config,
                given,
                paths,
            },
        });
    }

    /// The form's config with the applied capture rules, so the engine's first filters and a
    /// saved `config.json` match the sidebar.
    fn config_with_rules(&self) -> (ToolConfig, SensitiveValues) {
        let (mut config, given) = self.form.to_config();
        config.capture.filters = self.read.rules_applied.clone();
        (config, given)
    }

    fn read_input(&mut self, input: ReadInput, out: &mut Outcome) {
        let paths = self.data_paths();
        let read = self.read.apply(input, &self.connection, paths);
        for (kind, text) in read.toasts {
            self.toast(kind, text);
        }
        out.effects
            .extend(read.effects.into_iter().map(Effect::Read));
        out.changed = read.changed;
        if read.rows_landed {
            out.motion = Motion::Instant;
        }
    }

    fn archive_input(&mut self, input: ArchiveInput, out: &mut Outcome) {
        let paths = self.data_paths();
        let archive = self.archive.apply(input, &paths);
        for (kind, text) in archive.toasts {
            self.toast(kind, text);
        }
        out.effects
            .extend(archive.effects.into_iter().map(Effect::Archive));
        out.changed = archive.changed;
        if let Some(text) = archive.bulk_failure {
            self.failure_toast(Section::Export, text);
        }
        if archive.run_failed && self.window_closed {
            info!("a bulk export failed with the window closed; reopening it");
            self.reopen(out);
        }
    }

    fn disconnect_pressed(&mut self, out: &mut Outcome) {
        match std::mem::take(&mut self.connection) {
            ConnectionState::Connecting {
                attempt,
                session: None,
                ..
            } => {
                info!(attempt, "connect cancelled while preparing");
                self.abandon(attempt);
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
        if self.profile_loading() {
            debug!("Save config ignored: a profile is loading");
            out.changed = false;
            return;
        }
        if !self.form.issues().is_empty() {
            self.show_issues = true;
            return;
        }
        let (config, given) = self.config_with_rules();
        let filled = match self.home() {
            ConfigHome::Profile(_) => match new_profile::filled_names(&config, &given) {
                Ok(filled) => filled,
                Err(text) => {
                    self.config_save_failed(text);
                    return;
                }
            },
            ConfigHome::CurrentFolder(_) => None,
        };
        let config = match filled {
            Some(filled) => {
                self.names_filled(&filled.names);
                filled.config
            }
            None => config,
        };
        self.save_status = SaveStatus::Saving(SaveTo::Home(self.form.pending_save()));
        out.effects.push(Effect::SaveConfig(SaveRequest {
            config,
            given,
            dir: self.home().dir().to_path_buf(),
            paths: self.data_paths(),
        }));
    }

    fn config_saved(&mut self, result: Result<(), String>, out: &mut Outcome) {
        let pending = match std::mem::replace(&mut self.save_status, SaveStatus::Idle) {
            SaveStatus::Saving(SaveTo::Home(pending)) => Some(pending),
            creating @ SaveStatus::Saving(SaveTo::NewProfile { .. }) => {
                match &result {
                    Ok(()) => debug!("a config.json save answered while a profile saves; dropped"),
                    Err(text) => debug!(
                        error = %text,
                        "a failed config.json save answered while a profile saves; dropped"
                    ),
                }
                self.save_status = creating;
                out.changed = false;
                return;
            }
            SaveStatus::Idle | SaveStatus::Saved | SaveStatus::Failed(_) => None,
        };
        match result {
            Ok(()) => {
                match &pending {
                    Some(pending) => self.form.settle_after_save(pending),
                    None => debug!("a save finished that this state did not start"),
                }
                self.save_status = SaveStatus::Saved;
                self.send.refresh_destination(&self.form);
            }
            Err(text) => self.config_save_failed(text),
        }
    }

    /// Writes `names` into the form's blank name fields as typed text, so the form matches the
    /// `config.json` the save writes and a second save keeps them.
    fn names_filled(&mut self, names: &Names) {
        self.form
            .fill_names(Some(&names.namespace), Some(&names.event_hub));
        self.send.refresh_destination(&self.form);
        info!(
            namespace = names.namespace.as_str(),
            event_hub = names.event_hub.as_str(),
            "Save config fills the names the typed connection string gives"
        );
    }

    /// A Save config that failed with `text`: a Danger toast and the footer's "Not saved".
    fn config_save_failed(&mut self, text: String) {
        warn!(error = %text, "saving config.json failed");
        self.toast(
            ToastKind::Danger,
            format!("Could not save config.json: {text}"),
        );
        self.save_status = SaveStatus::Failed(text);
    }

    /// Why the data folder cannot change now, or `None` when it can: nothing may be using the
    /// current folder. A connect attempt abandoned while preparing holds it until its prepare
    /// answers. Two holders are left out: a search page in flight is discarded instead, and a
    /// session that started after its attempt was abandoned lets go of its archive handle when
    /// its stop ends, after the prepare answered.
    pub fn data_folder_lock(&self) -> Option<&'static str> {
        match &self.connection {
            ConnectionState::Disconnected | ConnectionState::Failed(_) => {}
            ConnectionState::Connecting { .. }
            | ConnectionState::Connected { .. }
            | ConnectionState::Stopping { .. } => {
                return Some("The data folder can change once the session ends.");
            }
        }
        if !self.abandoned_attempts.is_empty() {
            return Some("The data folder can change once the cancelled connection stops.");
        }
        if self.save_status.is_saving() {
            return Some("The data folder can change once the save finishes.");
        }
        if self.archive.bulk.is_running() {
            return Some("The data folder can change once the bulk export ends.");
        }
        if self.read.export_running() || self.archive.inspector.export.is_running() {
            return Some("The data folder can change once the message export ends.");
        }
        if self.send.is_running() {
            return Some("The data folder can change once the send ends.");
        }
        None
    }

    fn data_folder_picked(&mut self, folder: PathBuf, out: &mut Outcome) {
        if let Some(reason) = self.data_folder_lock() {
            debug!(reason, "data folder change refused");
            out.changed = false;
            return;
        }
        if self.profile_loading() {
            debug!("data folder change ignored: a profile is loading");
            out.changed = false;
            return;
        }
        self.form.data_folder = folder.display().to_string();
        self.form.forget_stored_secrets();
        let paths = self.data_paths();
        self.change_data_folder(paths, out);
    }

    /// Moves everything that follows the data folder to `paths`: the send destination, the
    /// archive results, and the store.
    fn change_data_folder(&mut self, paths: DataPaths, out: &mut Outcome) {
        self.send.refresh_destination(&self.form);
        self.archive.folder_changed();
        info!(folder = %paths.base().display(), "data folder changed");
        out.effects.push(Effect::SwitchDataFolder(paths));
    }

    /// Records `attempt`, abandoned while preparing: its prepare may still open the archive.
    fn abandon(&mut self, attempt: u64) {
        self.abandoned_attempts.push(attempt);
        debug!(
            attempt,
            abandoned = self.abandoned_attempts.len(),
            "an abandoned attempt holds the data folder until it answers"
        );
    }

    /// Forgets abandoned `attempt` once its prepare answered; true when it was recorded.
    fn abandoned_answered(&mut self, attempt: u64) -> bool {
        let Some(index) = self.abandoned_attempts.iter().position(|a| *a == attempt) else {
            debug!(attempt, "an answer for an attempt no one waits on");
            return false;
        };
        self.abandoned_attempts.remove(index);
        debug!(
            attempt,
            abandoned = self.abandoned_attempts.len(),
            "an abandoned attempt answered"
        );
        true
    }

    /// The connect attempts abandoned while preparing that have not answered yet.
    pub fn abandoned_attempts(&self) -> &[u64] {
        &self.abandoned_attempts
    }

    fn window_closed(&mut self, at: DateTime<Utc>, out: &mut Outcome) {
        self.window_closed = true;
        self.closed_at = Some(at);
        self.close_sheet();
        self.details = None;
        self.fold_toasts();
        self.archive.window_closed();
        self.send.request_cancel(CancelReason::WindowClosed);
        match std::mem::take(&mut self.connection) {
            ConnectionState::Connecting {
                attempt,
                session: None,
                ..
            } => {
                info!(
                    attempt,
                    "window closed while preparing; the attempt is abandoned"
                );
                self.abandon(attempt);
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
        out.changed = self.tick_toasts();
        out.changed |= self.tick_details(now);
        let connected = match self.connection {
            ConnectionState::Connected { .. } => true,
            ConnectionState::Disconnected
            | ConnectionState::Connecting { .. }
            | ConnectionState::Stopping { .. }
            | ConnectionState::Failed(_) => false,
        };
        out.changed |= self.read.tick(now, connected);
        out.changed |= self.archive.tick(now);
        out.changed |= self.send.tick(now);
        if out.changed {
            out.motion = Motion::Animated;
        }

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

    fn appearance_changed(&mut self, dark: bool, out: &mut Outcome) {
        if self.system_dark == Some(dark) {
            out.changed = false;
            return;
        }
        info!(dark, "using the system appearance");
        self.system_dark = Some(dark);
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
        let send_running = self.send.is_running();
        self.send.request_cancel(CancelReason::WindowClosed);
        warn!(
            send_running,
            "force quit: the session's at-end pass was abandoned, and a running send is not awaited"
        );
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
                self.send.refresh_destination(&self.form);
                self.connection = ConnectionState::Connecting {
                    attempt,
                    session: Some(session),
                    fatal: None,
                };
            }
            (true, Err(text)) => {
                warn!(attempt, error = %text, "the session could not start");
                self.failure_toast(Section::Read, format!("Could not connect: {text}"));
                self.connection = ConnectionState::Failed(Failure::starting(text));
            }
            (false, Ok(session)) => {
                debug!(
                    attempt,
                    "a session started after its attempt was abandoned; stopping it"
                );
                out.effects.push(Effect::StopOrphan(session));
                out.changed = self.abandoned_answered(attempt);
            }
            (false, Err(text)) => {
                debug!(attempt, error = %text, "an abandoned attempt failed");
                out.changed = self.abandoned_answered(attempt);
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
        let at = delivery.at;
        self.read.delivered(at, delivery.stats);
        for event in delivery.events {
            match event {
                EngineEvent::Started { partitions } => self.started(partitions, out),
                EngineEvent::Message(message) => self.read.message(*message),
                EngineEvent::Error {
                    partition_id,
                    error,
                    fatal,
                } => {
                    if fatal {
                        debug!(partition = ?partition_id, "the engine reported a fatal error");
                        if let ConnectionState::Connecting { fatal, .. } = &mut self.connection {
                            *fatal = Some(error.clone());
                        }
                    } else {
                        warn!(partition = ?partition_id, error = %error, "a partition reported an error");
                    }
                    self.session_log(at, LogKind::Error, partition_id, error, out);
                }
                EngineEvent::PartitionEnded { partition_id, end } => {
                    let text = format!("Partition {partition_id} {end}");
                    self.session_log(at, LogKind::PartitionEnded, Some(partition_id), text, out);
                }
                EngineEvent::Skipped { .. } => {}
            }
        }
    }

    fn session_log(
        &mut self,
        at: DateTime<Utc>,
        kind: LogKind,
        partition: Option<String>,
        text: String,
        out: &mut Outcome,
    ) {
        self.read.log(at, kind, partition, text, self.pill_open);
        out.changed = true;
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
                out.motion = Motion::Animated;
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
        self.read.end_session();
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
                self.failure_toast(Section::Read, text.clone());
                self.connection = ConnectionState::Failed(Failure { stage, text });
                if self.window_closed {
                    self.reopen(out);
                }
            }
        }
        self.still_finishing = false;
    }

    /// Toasts a failure unless `shown_by`, the section whose content shows it in place (the
    /// Read empty state, the bulk export bar), is the one in the window: a toast there would
    /// repeat it and cover that section's controls in a small window. A failure that lands
    /// with the window closed reopens it on the same section, so the section decides then too.
    fn failure_toast(&mut self, shown_by: Section, text: String) {
        if self.section != shown_by {
            self.toast(ToastKind::Danger, text);
        }
    }

    fn reopen(&mut self, out: &mut Outcome) {
        self.window_closed = false;
        self.closed_at = None;
        out.effects.push(Effect::ReopenWindow);
    }

    /// Quits once the window is closed and neither a session, a save, an export, nor a send is
    /// left running. Asks for the quit once; the quit path then runs to the end on its own.
    fn quit_rule(&mut self, out: &mut Outcome) {
        if self.quit_requested
            || !self.window_closed
            || self.connection.session().is_some()
            || self.save_status.is_saving()
            || self.read.export_running()
            || self.archive.busy()
            || self.send.is_running()
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
