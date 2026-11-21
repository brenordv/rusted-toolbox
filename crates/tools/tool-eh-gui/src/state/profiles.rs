//! Profiles in the app state: the folder `config.json` is read from and saved to (the home),
//! the profiles the picker lists, the one load a pick (or a saved new profile) starts, and what
//! that load left to say.
//!
//! A pick only starts a load; the answer changes the home, and only through `apply_load`, when
//! nothing holds the data folder. Request ids only go up, so an answer to an older list or load
//! can never pass for the current one.

use super::{AppState, ConnectionState, Effect, Outcome, SaveStatus, ToastKind};
use crate::form::{ConnectForm, SecretKind};
use crate::read::rows::display_line;
use shared_eventhub_core::filters::CaptureFilters;
use shared_eventhub_persistence::config::{ConfigLoad, ExportSettings};
use shared_eventhub_persistence::paths::DataPaths;
use shared_eventhub_persistence::workspace::{ConfigHome, ProfileEntry, Workspace};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// The picker's name for the folder eh-gui started in.
pub const LAUNCH_FOLDER: &str = "Launch folder";
/// Why a state built without a launch has no workspace.
pub const NO_WORKSPACE: &str = "Profiles are not set up in this window";
/// The caption a workspace with no profile folders shows.
pub const NO_PROFILES: &str = "No saved profiles yet.";
const FOREIGN_CAPTION: &str = "A config.json here belongs to another tool; it was not loaded";

/// The workspace's profiles as last listed. No state is final: a new listing starts from any
/// of them.
#[derive(Debug, Default)]
pub enum ProfileList {
    #[default]
    NotLoaded,
    /// A listing runs; `previous` keeps the last entries on screen meanwhile.
    Loading {
        request: u64,
        previous: Option<Vec<ProfileEntry>>,
    },
    Loaded(Vec<ProfileEntry>),
    /// The workspace could not be listed; the text says why.
    Failed(String),
}

impl ProfileList {
    /// The entries to show: the listed ones, or the previous ones while a listing runs.
    pub fn entries(&self) -> Option<&[ProfileEntry]> {
        match self {
            Self::Loaded(entries) => Some(entries),
            Self::Loading { previous, .. } => previous.as_deref(),
            Self::NotLoaded | Self::Failed(_) => None,
        }
    }
}

/// The one profile load a pick starts; the latest pick wins.
#[derive(Debug, Default)]
pub enum ProfileSwitch {
    #[default]
    Idle,
    /// `config.json` of `to` is being read, for load `request`.
    Loading { request: u64, to: ConfigHome },
}

impl ProfileSwitch {
    /// Whether a load runs.
    pub fn is_loading(&self) -> bool {
        match self {
            Self::Idle => false,
            Self::Loading { .. } => true,
        }
    }
}

/// What a pick's outcome has to say when it changed nothing or dropped a typed secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileNote {
    /// A typed, unsaved secret went with the replaced form.
    SecretCleared(SecretKind),
    /// The profile could not be opened; the home did not change.
    LoadFailed { name: String, text: String },
    /// The answer came while something held the data folder; nothing changed.
    NotSwitched(&'static str),
    /// A new profile was saved under this name and is opening.
    Created(String),
    /// A new profile was saved while something held the data folder; the home did not change.
    CreatedNotSwitched { name: String, reason: &'static str },
}

impl ProfileNote {
    /// The note's sentence, with control and bidirectional characters from disk shown as
    /// spaces.
    pub fn text(&self) -> String {
        let text = match self {
            Self::SecretCleared(SecretKind::ConnectionString) => {
                "The connection string you typed was cleared; it wasn't saved.".to_owned()
            }
            Self::SecretCleared(SecretKind::ClientSecret) => {
                "The client secret you typed was cleared; it wasn't saved.".to_owned()
            }
            Self::LoadFailed { name, text } => format!(
                "Couldn't open profile {name}: {}. Pick another profile or fix its config.json.",
                text.trim_end_matches('.')
            ),
            Self::NotSwitched(reason) => format!("Not switched. {reason}"),
            Self::Created(name) => format!("Saved profile {name}."),
            Self::CreatedNotSwitched { name, reason } => format!(
                "Saved profile {name}, with what you typed. Not switched. {reason} Pick it under \
                 Profile in the connection sheet to use it."
            ),
        };
        display_line(text.chars())
    }

    /// Whether the note reports a failure (drawn in the danger color).
    pub fn is_failure(&self) -> bool {
        match self {
            Self::LoadFailed { .. } => true,
            Self::SecretCleared(_)
            | Self::NotSwitched(_)
            | Self::Created(_)
            | Self::CreatedNotSwitched { .. } => false,
        }
    }

    /// How the note toasts when it lands with the sheet closed: its kind and its own title.
    fn toast(&self) -> (ToastKind, Option<&'static str>) {
        match self {
            Self::LoadFailed { .. } => (ToastKind::Danger, None),
            Self::NotSwitched(_) | Self::CreatedNotSwitched { .. } => {
                (ToastKind::Info, Some("Profile not switched"))
            }
            Self::SecretCleared(_) => (ToastKind::Info, Some("Profile switched")),
            Self::Created(_) => (ToastKind::Info, Some("Profile saved")),
        }
    }
}

/// What a `config.json` gives a home: the form, the capture rules, the export settings, and
/// the caption saying what happened to the file (never a value from it).
#[derive(Debug)]
pub struct Recalled {
    pub form: ConnectForm,
    pub rules: CaptureFilters,
    pub export: ExportSettings,
    pub caption: Option<String>,
}

impl Recalled {
    fn defaults(caption: Option<String>) -> Self {
        Self {
            form: ConnectForm::default(),
            rules: CaptureFilters::default(),
            export: ExportSettings::default(),
            caption,
        }
    }
}

/// The form, rules, export settings, and caption `load` (the reading of `home`'s
/// `config.json`) gives. A launch folder's config marks the form's names as recalled from it.
/// A failed read logs once and leaves the defaults.
pub fn recalled(home: &ConfigHome, load: Result<ConfigLoad, String>) -> Recalled {
    match load {
        Ok(ConfigLoad::Missing) => Recalled::defaults(None),
        Ok(ConfigLoad::Foreign) => Recalled::defaults(Some(FOREIGN_CAPTION.to_owned())),
        Ok(ConfigLoad::Loaded(config)) => {
            let (mut form, loaded) = ConnectForm::from_config(&config);
            let source = match home {
                ConfigHome::CurrentFolder(_) => {
                    form.names_from_launch_folder = true;
                    "config.json".to_owned()
                }
                ConfigHome::Profile(profile) => {
                    format!("profile {}", display_line(profile.name().chars()))
                }
            };
            Recalled {
                form,
                rules: config.capture.filters.clone(),
                export: config.storage.export.clone(),
                caption: Some(format!("Loaded from {source}: {}", loaded.join(", "))),
            }
        }
        Err(error) => {
            warn!(error = %error, "config.json was not loaded");
            Recalled::defaults(Some(format!("config.json was not loaded: {error}")))
        }
    }
}

/// The picker's label for `home`: [`LAUNCH_FOLDER`] or the profile's name as shown.
pub fn home_label(home: &ConfigHome) -> String {
    match home {
        ConfigHome::CurrentFolder(_) => LAUNCH_FOLDER.to_owned(),
        ConfigHome::Profile(profile) => display_line(profile.name().chars()),
    }
}

/// `home` as a log field: "launch folder" or the profile's name.
fn log_label(home: &ConfigHome) -> &str {
    match home {
        ConfigHome::CurrentFolder(_) => "launch folder",
        ConfigHome::Profile(profile) => profile.name(),
    }
}

/// The app's profile state.
#[derive(Debug)]
pub(super) struct Profiles {
    /// The folder eh-gui started in; the picker's "Launch folder".
    launch_folder: PathBuf,
    /// Where `config.json` is read from and saved to. Written only by `set_home`.
    home: ConfigHome,
    /// The workspace, or why there is none (no home folder, a bad `EH_WORKSPACE`).
    workspace: Result<Workspace, String>,
    list: ProfileList,
    switch: ProfileSwitch,
    /// Set only while the sheet is open; cleared by the next pick and by a sheet close.
    note: Option<ProfileNote>,
    next_request: u64,
}

impl Profiles {
    /// Profiles over the current folder, with no workspace.
    pub(super) fn new(launch_folder: PathBuf) -> Self {
        Self {
            home: ConfigHome::CurrentFolder(launch_folder.clone()),
            launch_folder,
            workspace: Err(NO_WORKSPACE.to_owned()),
            list: ProfileList::NotLoaded,
            switch: ProfileSwitch::Idle,
            note: None,
            next_request: 1,
        }
    }
}

impl AppState {
    /// The folder `config.json` is read from and saved to.
    pub fn home(&self) -> &ConfigHome {
        &self.profiles.home
    }

    /// The folder eh-gui started in.
    pub fn launch_folder(&self) -> &Path {
        &self.profiles.launch_folder
    }

    /// The workspace, or why there is none.
    pub fn workspace(&self) -> Result<&Workspace, &str> {
        self.profiles.workspace.as_ref().map_err(String::as_str)
    }

    /// The workspace's profiles as last listed.
    pub fn profile_list(&self) -> &ProfileList {
        &self.profiles.list
    }

    /// The pending profile load, if any.
    pub fn profile_switch(&self) -> &ProfileSwitch {
        &self.profiles.switch
    }

    /// What the last pick left to say, while the sheet is open.
    pub fn profile_note(&self) -> Option<&ProfileNote> {
        self.profiles.note.as_ref()
    }

    /// The home a picker label names: the launch folder, a listed profile that loads, or the
    /// current home; `None` for a label that names none of them.
    pub fn home_for_label(&self, label: &str) -> Option<ConfigHome> {
        if label == LAUNCH_FOLDER {
            return Some(ConfigHome::CurrentFolder(
                self.profiles.launch_folder.clone(),
            ));
        }
        if home_label(&self.profiles.home) == label {
            return Some(self.profiles.home.clone());
        }
        self.profiles
            .list
            .entries()?
            .iter()
            .filter(|entry| entry.state.is_ready())
            .find(|entry| display_line(entry.profile.name().chars()) == label)
            .map(|entry| ConfigHome::Profile(entry.profile.clone()))
    }

    /// The data folder the form names, from the home: under a profile, a relative folder sits
    /// in the profile folder; under the launch folder it stays relative.
    pub fn data_paths(&self) -> DataPaths {
        self.profiles.home.data_paths(self.form.data_folder.trim())
    }

    /// Starts on `launch_folder` with `workspace`: the home is the launch folder and nothing is
    /// listed yet. Call it before the first frame: it moves the home without the lock check or
    /// a data folder switch.
    pub(crate) fn set_launch(
        &mut self,
        launch_folder: PathBuf,
        workspace: Result<Workspace, String>,
    ) {
        self.set_home(ConfigHome::CurrentFolder(launch_folder.clone()));
        self.profiles.launch_folder = launch_folder;
        self.profiles.workspace = workspace;
        self.profiles.list = ProfileList::NotLoaded;
    }

    fn set_home(&mut self, home: ConfigHome) {
        self.profiles.home = home;
    }

    pub(super) fn next_profile_request(&mut self) -> u64 {
        let request = self.profiles.next_request;
        self.profiles.next_request += 1;
        request
    }

    /// Lists the workspace again, keeping the entries shown meanwhile; nothing without one.
    pub(super) fn refresh_profiles(&mut self, out: &mut Outcome) {
        let workspace = match &self.profiles.workspace {
            Ok(workspace) => workspace.clone(),
            Err(reason) => {
                debug!(reason = %reason, "no workspace to list");
                return;
            }
        };
        let request = self.next_profile_request();
        let previous = match std::mem::take(&mut self.profiles.list) {
            ProfileList::Loaded(entries) => Some(entries),
            ProfileList::Loading { previous, .. } => previous,
            ProfileList::NotLoaded | ProfileList::Failed(_) => None,
        };
        self.profiles.list = ProfileList::Loading { request, previous };
        debug!(request, "listing the profiles");
        out.effects
            .push(Effect::ListProfiles { request, workspace });
    }

    pub(super) fn profiles_listed(
        &mut self,
        request: u64,
        result: Result<Vec<ProfileEntry>, String>,
        out: &mut Outcome,
    ) {
        let current = match &self.profiles.list {
            ProfileList::Loading {
                request: pending, ..
            } => *pending == request,
            ProfileList::NotLoaded | ProfileList::Loaded(_) | ProfileList::Failed(_) => false,
        };
        if !current {
            debug!(request, "a stale profile listing was dropped");
            out.changed = false;
            return;
        }
        self.profiles.list = match result {
            Ok(entries) => {
                debug!(request, count = entries.len(), "profiles listed");
                ProfileList::Loaded(entries)
            }
            Err(text) => {
                warn!(error = %text, "the profiles could not be listed");
                ProfileList::Failed(text)
            }
        };
    }

    /// Starts loading `to`'s `config.json`, unless something holds the data folder; picking
    /// the current home drops a pending load.
    pub(super) fn profile_picked(&mut self, to: ConfigHome, out: &mut Outcome) {
        if let Some(reason) = self.data_folder_lock() {
            debug!(reason, "profile pick refused");
            out.changed = false;
            return;
        }
        if to == self.profiles.home {
            match std::mem::take(&mut self.profiles.switch) {
                ProfileSwitch::Loading { request, .. } => {
                    debug!(
                        request,
                        "the current home was picked; the pending load is dropped"
                    );
                }
                ProfileSwitch::Idle => {
                    debug!("the current home was picked; nothing to load");
                    out.changed = false;
                }
            }
            return;
        }
        self.start_load(to, out);
    }

    /// Starts loading `to`'s `config.json`, even when `to` is the current home, in place of
    /// any pending load; the answer changes the home only through `apply_load`.
    pub(super) fn start_load(&mut self, to: ConfigHome, out: &mut Outcome) {
        let request = self.next_profile_request();
        let dir = to.dir().to_path_buf();
        debug!(request, profile = log_label(&to), "loading a profile");
        self.profiles.switch = ProfileSwitch::Loading { request, to };
        self.profiles.note = None;
        out.effects.push(Effect::LoadProfile { request, dir });
    }

    pub(super) fn profile_loaded(
        &mut self,
        request: u64,
        result: Result<ConfigLoad, String>,
        out: &mut Outcome,
    ) {
        let to = match std::mem::take(&mut self.profiles.switch) {
            ProfileSwitch::Loading {
                request: pending,
                to,
            } if pending == request => to,
            pending @ ProfileSwitch::Loading { .. } => {
                self.profiles.switch = pending;
                debug!(request, "a stale profile load was dropped");
                out.changed = false;
                return;
            }
            ProfileSwitch::Idle => {
                debug!(request, "a profile load landed with none pending; dropped");
                out.changed = false;
                return;
            }
        };
        if let Some(reason) = self.data_folder_lock() {
            info!(
                reason,
                profile = log_label(&to),
                "a profile loaded while the data folder was in use; not switched"
            );
            self.set_profile_note(ProfileNote::NotSwitched(reason));
            return;
        }
        self.apply_load(to, result, out);
    }

    /// The only place a pick changes anything: replaces the form, the rules, and the export
    /// options with what `to`'s `config.json` gives, makes `to` the home, resets what belonged
    /// to the old form, and moves the data folder. A profile that can't be opened changes
    /// nothing but the note; the launch folder always opens, with the startup captions.
    fn apply_load(
        &mut self,
        to: ConfigHome,
        result: Result<ConfigLoad, String>,
        out: &mut Outcome,
    ) {
        if let Some((name, text)) = profile_failure(&to, &result) {
            warn!(profile = %name, error = %text, "the profile could not be opened");
            self.set_profile_note(ProfileNote::LoadFailed { name, text });
            return;
        }
        let typed = self.form.typed_secret();
        let Recalled {
            form,
            rules,
            export,
            caption,
        } = recalled(&to, result);
        self.form = form;
        self.read
            .reseed(&rules, export.format, export.include_metadata);
        self.archive.reseed(&export);
        let profile = log_label(&to).to_owned();
        self.set_home(to);
        self.save_status = SaveStatus::Idle;
        match &self.connection {
            ConnectionState::Failed(_) => self.connection = ConnectionState::Disconnected,
            ConnectionState::Disconnected
            | ConnectionState::Connecting { .. }
            | ConnectionState::Connected { .. }
            | ConnectionState::Stopping { .. } => {}
        }
        self.show_issues = false;
        self.clear_new_profile_issues();
        self.config_caption = caption;
        let paths = self.data_paths();
        self.change_data_folder(paths, out);
        if let Some(kind) = typed {
            self.set_profile_note(ProfileNote::SecretCleared(kind));
        }
        info!(profile = %profile, "profile switched");
    }

    /// Shows `note` under the picker while the sheet is open; with the sheet closed nothing
    /// would show it, so it toasts instead.
    pub(super) fn set_profile_note(&mut self, note: ProfileNote) {
        if self.sheet.is_some() {
            self.profiles.note = Some(note);
            return;
        }
        let (kind, title) = note.toast();
        self.toast_titled(kind, title, note.text());
    }

    /// Drops the note.
    pub(super) fn clear_profile_note(&mut self) {
        self.profiles.note = None;
    }

    /// Whether a profile load is pending.
    pub(super) fn profile_loading(&self) -> bool {
        self.profiles.switch.is_loading()
    }
}

/// The profile's name and why it can't be opened, when `to` is a profile and `result` gives no
/// config; `None` for a config, and always for the launch folder.
fn profile_failure(
    to: &ConfigHome,
    result: &Result<ConfigLoad, String>,
) -> Option<(String, String)> {
    let profile = match to {
        ConfigHome::CurrentFolder(_) => return None,
        ConfigHome::Profile(profile) => profile,
    };
    let text = match result {
        Ok(ConfigLoad::Loaded(_)) => return None,
        Ok(ConfigLoad::Missing) => "its config.json is gone".to_owned(),
        Ok(ConfigLoad::Foreign) => "its config.json belongs to another tool".to_owned(),
        Err(text) => text.clone(),
    };
    Some((profile.name().to_owned(), text))
}

#[cfg(test)]
mod tests;
