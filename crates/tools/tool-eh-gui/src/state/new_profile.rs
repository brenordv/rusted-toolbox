//! The connection sheet's Save as profile row in the app state: the name typed, why the last
//! press could not start, the press that saves the form into a new profile folder, and what its
//! answer does. A save that lands opens the new profile through the same load a pick starts.

use super::profiles::ProfileNote;
use super::{AppState, Effect, Outcome, SaveStatus, SaveTo, ToastKind};
use crate::form::{RecalledFields, SecretKind};
use crate::read::rows::display_line;
use crate::services::CreateRequest;
use shared_eventhub_persistence::paths::{DEFAULT_DATA_FOLDER, plain_relative};
use shared_eventhub_persistence::workspace::{ConfigHome, Profile, ProfileName, Workspace};
use tracing::{debug, info, warn};

/// What a namespace or event hub field says while it holds, unedited, what the launch folder's
/// `config.json` gave, after a Save profile press asked for it again.
pub const RECALLED_FIELD: &str =
    "Came from the launch folder's config.json. Type it again to save it in a profile.";
const EMPTY_NAME: &str = "Type a name for the new profile.";
const FORM_ISSUES: &str = "Not saved: fix the fields marked below.";

/// Whether the Save as profile row shows, and why its last press could not start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewProfileRow {
    Closed,
    /// `name_issue` is about the name (it marks the field); `row_issue` is about the rest.
    Open {
        name_issue: Option<String>,
        row_issue: Option<RowIssue>,
    },
}

/// Why the last press could not start, past the name. The line it shows is drawn from the form
/// as it is now, so fixing the cause clears the line at the next frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowIssue {
    /// The form has field issues.
    FormIssues,
    /// The chosen method's secret is the one in the current data folder's secrets file.
    StoredSecret,
    /// The namespace or event hub still holds what the launch folder's `config.json` gave.
    RecalledNames,
}

/// A control of the Save as profile row that focus moves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileFocus {
    /// The profile name field.
    NameField,
    /// Save as profile…, at the end of the Saves to row.
    SaveAsButton,
}

impl AppState {
    /// The Save as profile row.
    pub fn new_profile_row(&self) -> &NewProfileRow {
        &self.new_profile
    }

    /// The row's issue line as the form stands now; `None` while the row is closed or holds no
    /// row issue, and once the issue's cause is gone.
    pub fn new_profile_issue(&self) -> Option<String> {
        let issue = match &self.new_profile {
            NewProfileRow::Open {
                row_issue: Some(issue),
                ..
            } => *issue,
            NewProfileRow::Open {
                row_issue: None, ..
            }
            | NewProfileRow::Closed => return None,
        };
        match issue {
            RowIssue::FormIssues => {
                (!self.form.issues().is_empty()).then(|| FORM_ISSUES.to_owned())
            }
            RowIssue::StoredSecret => self.form.stored_secret().map(stored_secret_line),
            RowIssue::RecalledNames => recalled_line(self.form.recalled_fields()),
        }
    }

    /// The names the sheet marks as recalled from the launch folder: those the form still
    /// holds unedited, while the row's issue asks for them again; none otherwise.
    pub fn recall_marks(&self) -> RecalledFields {
        match &self.new_profile {
            NewProfileRow::Open {
                row_issue: Some(RowIssue::RecalledNames),
                ..
            } => self.form.recalled_fields(),
            NewProfileRow::Open {
                row_issue: Some(RowIssue::FormIssues | RowIssue::StoredSecret) | None,
                ..
            }
            | NewProfileRow::Closed => RecalledFields::default(),
        }
    }

    /// The data folder a profile saved now names: the form's when it is plain relative, else
    /// [`DEFAULT_DATA_FOLDER`], so the archive and the secrets file sit in the profile folder.
    pub fn new_profile_data_folder(&self) -> &str {
        let folder = self.form.data_folder.trim();
        if plain_relative(folder) {
            folder
        } else {
            DEFAULT_DATA_FOLDER
        }
    }

    /// The row control focus should move to, once.
    pub(crate) fn take_profile_focus(&mut self) -> Option<ProfileFocus> {
        self.pending_focus.take()
    }

    /// Opens the row with an empty name and asks for focus in it, unless the row is open
    /// already, the sheet is closed, there is no workspace, the data folder is held, or a
    /// profile loads.
    pub(super) fn new_profile_opened(&mut self, out: &mut Outcome) {
        if let Some(reason) = self.open_refusal() {
            debug!(reason, "Save as profile ignored");
            out.changed = false;
            return;
        }
        self.new_profile_name.clear();
        self.new_profile = NewProfileRow::Open {
            name_issue: None,
            row_issue: None,
        };
        self.pending_focus = Some(ProfileFocus::NameField);
    }

    fn open_refusal(&self) -> Option<&'static str> {
        match &self.new_profile {
            NewProfileRow::Open { .. } => return Some("the row is open already"),
            NewProfileRow::Closed => {}
        }
        if self.sheet.is_none() {
            return Some("the sheet is closed");
        }
        if self.workspace().is_err() {
            return Some("there is no workspace");
        }
        if let Some(reason) = self.data_folder_lock() {
            return Some(reason);
        }
        if self.profile_loading() {
            return Some("a profile is loading");
        }
        None
    }

    /// Closes the row unless a profile is saving; focus then goes to Save as profile… while
    /// nothing holds the data folder (the button is enabled then).
    pub(super) fn new_profile_cancelled(&mut self, out: &mut Outcome) {
        match &self.new_profile {
            NewProfileRow::Closed => {
                debug!("Cancel ignored: the Save as profile row is closed");
                out.changed = false;
                return;
            }
            NewProfileRow::Open { .. } => {}
        }
        if let Some(name) = self.save_status.creating() {
            debug!(
                profile = name.as_str(),
                "Cancel ignored: the profile is saving"
            );
            out.changed = false;
            return;
        }
        self.new_profile = NewProfileRow::Closed;
        if self.data_folder_lock().is_none() {
            self.pending_focus = Some(ProfileFocus::SaveAsButton);
        }
    }

    /// An edit of the name drops the name's issue; the row's issue follows the form on its own.
    pub(super) fn new_profile_name_edited(&mut self, out: &mut Outcome) {
        out.changed = match &mut self.new_profile {
            NewProfileRow::Open { name_issue, .. } => name_issue.take().is_some(),
            NewProfileRow::Closed => false,
        };
    }

    /// Drops both issues: the form they judged is gone.
    pub(super) fn clear_new_profile_issues(&mut self) {
        match &mut self.new_profile {
            NewProfileRow::Open {
                name_issue,
                row_issue,
            } => {
                *name_issue = None;
                *row_issue = None;
            }
            NewProfileRow::Closed => {}
        }
    }

    /// Closes the row and drops any focus request for it.
    pub(super) fn close_new_profile_row(&mut self) {
        self.new_profile = NewProfileRow::Closed;
        self.pending_focus = None;
    }

    /// Checks the name, the form, its secrets, and its recalled names, stopping at the first
    /// that fails with its issue on the row; else saves what Save config would write, with the
    /// data folder kept inside the new profile folder.
    pub(super) fn create_pressed(&mut self, out: &mut Outcome) {
        let workspace = match self.create_target() {
            Ok(workspace) => workspace,
            Err(reason) => {
                debug!(reason, "Save profile ignored");
                out.changed = false;
                return;
            }
        };
        let name = match self.checked_name() {
            Ok(name) => name,
            Err(issue) => {
                debug!(issue = %issue, "Save profile refused: the name");
                // The row's issue stays: its line follows the form, so it shows only while its
                // cause does.
                let row_issue = match &self.new_profile {
                    NewProfileRow::Open { row_issue, .. } => *row_issue,
                    NewProfileRow::Closed => None,
                };
                self.new_profile = NewProfileRow::Open {
                    name_issue: Some(issue),
                    row_issue,
                };
                self.pending_focus = Some(ProfileFocus::NameField);
                return;
            }
        };
        if let Some(issue) = self.row_issue() {
            match issue {
                RowIssue::FormIssues => self.show_issues = true,
                RowIssue::StoredSecret | RowIssue::RecalledNames => {}
            }
            debug!(?issue, profile = name.as_str(), "Save profile refused");
            self.new_profile = NewProfileRow::Open {
                name_issue: None,
                row_issue: Some(issue),
            };
            return;
        }
        let (mut config, given) = self.config_with_rules();
        let folder = self.new_profile_data_folder().to_owned();
        if folder != config.storage.base_data_folder {
            debug!(
                "the form's data folder is not plain relative; the new profile uses the default"
            );
        }
        config.storage.base_data_folder = folder;
        let request = self.next_profile_request();
        debug!(
            request,
            profile = name.as_str(),
            "saving the form as a new profile"
        );
        self.save_status = SaveStatus::Saving(SaveTo::NewProfile {
            name: name.clone(),
            request,
        });
        self.new_profile = NewProfileRow::Open {
            name_issue: None,
            row_issue: None,
        };
        out.effects.push(Effect::CreateProfile(CreateRequest {
            request,
            workspace,
            name,
            config,
            given,
        }));
    }

    /// The workspace a press saves into, or why a press can't start now.
    fn create_target(&self) -> Result<Workspace, &'static str> {
        match &self.new_profile {
            NewProfileRow::Closed => return Err("the row is closed"),
            NewProfileRow::Open { .. } => {}
        }
        if self.save_status.is_saving() {
            return Err("a save is running");
        }
        if self.profile_loading() {
            return Err("a profile is loading");
        }
        if let Some(reason) = self.data_folder_lock() {
            return Err(reason);
        }
        self.workspace()
            .cloned()
            .map_err(|_| "there is no workspace")
    }

    /// The typed name as a profile name, or what the name field says about it: empty, against
    /// the name rules, or the name of a listed profile (ignoring ASCII case).
    fn checked_name(&self) -> Result<ProfileName, String> {
        let typed = self.new_profile_name.text.trim();
        if typed.is_empty() {
            return Err(EMPTY_NAME.to_owned());
        }
        let name = ProfileName::parse(typed).map_err(|error| format!("{error:#}"))?;
        let listed = self
            .profile_list()
            .entries()
            .unwrap_or_default()
            .iter()
            .find(|entry| entry.profile.name().eq_ignore_ascii_case(name.as_str()));
        let Some(entry) = listed else {
            return Ok(name);
        };
        let shown = display_line(entry.profile.name().chars());
        if entry.state.is_ready() {
            Err(format!(
                "A profile named {shown} already exists; pick it above."
            ))
        } else {
            Err(format!(
                "A profile named {shown} already exists but can't load; choose another name."
            ))
        }
    }

    /// What the form holds that a new profile must not take, in the order the press checks.
    fn row_issue(&self) -> Option<RowIssue> {
        if !self.form.issues().is_empty() {
            Some(RowIssue::FormIssues)
        } else if self.form.stored_secret().is_some() {
            Some(RowIssue::StoredSecret)
        } else if !self.form.recalled_fields().is_empty() {
            Some(RowIssue::RecalledNames)
        } else {
            None
        }
    }

    /// The answer to create `request`. A stale or crossed answer changes nothing.
    pub(super) fn profile_created(
        &mut self,
        request: u64,
        result: Result<Profile, String>,
        out: &mut Outcome,
    ) {
        let name = match std::mem::replace(&mut self.save_status, SaveStatus::Idle) {
            SaveStatus::Saving(SaveTo::NewProfile {
                name,
                request: pending,
            }) if pending == request => name,
            other @ (SaveStatus::Saving(SaveTo::NewProfile { .. } | SaveTo::Home(_))
            | SaveStatus::Idle
            | SaveStatus::Saved
            | SaveStatus::Failed(_)) => {
                self.save_status = other;
                match &result {
                    Ok(profile) => debug!(
                        request,
                        profile = profile.name(),
                        "a profile save answered that no create waits on; dropped"
                    ),
                    Err(text) => debug!(
                        request,
                        error = %text,
                        "a failed profile save answered that no create waits on; dropped"
                    ),
                }
                out.changed = false;
                return;
            }
        };
        match result {
            Ok(profile) => self.profile_saved(profile, out),
            Err(text) => {
                warn!(profile = name.as_str(), error = %text, "saving the profile failed");
                self.toast(
                    ToastKind::Danger,
                    format!(
                        "Could not save profile {}: {}",
                        name.as_str(),
                        display_line(text.chars())
                    ),
                );
                self.save_status = SaveStatus::Failed(text);
            }
        }
    }

    /// A saved `profile` holds the typed secrets now, so they leave the form; the row closes, the
    /// listing is read again while the sheet is open, and the profile opens through a load
    /// unless something holds the data folder.
    fn profile_saved(&mut self, profile: Profile, out: &mut Outcome) {
        self.form.clear_secrets();
        self.send.refresh_destination(&self.form);
        let row_was_open = match &self.new_profile {
            NewProfileRow::Open { .. } => true,
            NewProfileRow::Closed => false,
        };
        self.new_profile = NewProfileRow::Closed;
        self.new_profile_name.clear();
        if self.sheet.is_some() {
            self.refresh_profiles(out);
        }
        let name = profile.name().to_owned();
        match self.data_folder_lock() {
            None => {
                info!(profile = profile.name(), "profile created; opening it");
                self.start_load(ConfigHome::Profile(profile), out);
                self.set_profile_note(ProfileNote::Created(name));
                if row_was_open && self.sheet.is_some() {
                    self.pending_focus = Some(ProfileFocus::SaveAsButton);
                }
            }
            Some(reason) => {
                info!(
                    profile = profile.name(),
                    reason, "profile created; not switched"
                );
                self.set_profile_note(ProfileNote::CreatedNotSwitched { name, reason });
            }
        }
    }
}

fn stored_secret_line(kind: SecretKind) -> String {
    let field = match kind {
        SecretKind::ConnectionString => "Connection string",
        SecretKind::ClientSecret => "Client secret",
    };
    format!("A new profile keeps its own secrets. Press Replace beside {field} and type it again.")
}

fn recalled_line(fields: RecalledFields) -> Option<String> {
    let names = match (fields.namespace, fields.event_hub) {
        (true, true) => {
            return Some(
                "The namespace and event hub came from the launch folder's config.json. Type \
                 them again to save them in a profile."
                    .to_owned(),
            );
        }
        (true, false) => "The namespace",
        (false, true) => "The event hub",
        (false, false) => return None,
    };
    Some(format!(
        "{names} came from the launch folder's config.json. Type it again to save it in a \
         profile."
    ))
}

#[cfg(test)]
mod tests;
