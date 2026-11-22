//! The connection sheet's Profile group: which `config.json` the sheet edits, where Save config
//! writes, what the last pick left to say, the profiles that can't load, and the Save as profile
//! row that saves the form into a new profile.

use crate::app::dispatch;
use crate::design::parts;
use crate::design::tokens::{SHEET_FIELD_WIDTH, SPACE_SM, SPACE_XS};
use crate::form::SecretKind;
use crate::read::rows::{display_line, display_path};
use crate::state::{
    AppState, Input, LAUNCH_FOLDER, NO_PROFILES, NewProfileRow, ProfileDestination, ProfileFocus,
    ProfileList, ProfileNote, ProfileSwitch, RowLine, error_preview, home_label,
};
use crate::views::{INVALID_CONNECTION_STRING, details};
use rune::{El, Key, KeyEvent, LiveRegion, TextEdit, Tx, col, row, spacer};
use rune_kit::{Button, ButtonVariant, Kit, Select, SelectAppearance, TextInput};
use shared_eventhub_persistence::config::CONFIG_FILE_NAME;
use tracing::debug;

/// The group's key; its accessible name is "Profile".
pub const GROUP_KEY: &str = "profile-group";
/// The picker's key; its trigger is keyed `{PICKER_KEY}-trigger`.
pub const PICKER_KEY: &str = "profile-picker";
/// The note line's key: a polite live region.
pub const NOTE_KEY: &str = "profile-note";
/// Save as profile…, at the end of the Saves to row.
pub const NEW_PROFILE_KEY: &str = "new-profile";
/// The Save as profile row: a group named "New profile".
pub const NEW_PROFILE_ROW_KEY: &str = "new-profile-row";
/// The profile name field.
pub const NEW_PROFILE_NAME_KEY: &str = "new-profile-name";
/// The name's issue line: a polite live region.
pub const NAME_ISSUE_KEY: &str = "new-profile-name-issue";
/// The row's issue or confirm line: a polite live region.
pub const ROW_ISSUE_KEY: &str = "new-profile-issue";
/// The row's Cancel.
pub const NEW_PROFILE_CANCEL_KEY: &str = "new-profile-cancel";
/// The row's Save profile.
pub const NEW_PROFILE_SAVE_KEY: &str = "new-profile-save";
const NOTE_DETAILS_KEY: &str = "profile-note-details";
const PROFILE: &str = "Profile";
const PROFILE_NAME: &str = "Profile name";
const EVENT_HUB: &str = "Event hub";
const NO_DESTINATION: &str = "None yet; eh asks for one when sending";

/// The group: its label, the picker (when there is more than the launch folder to pick) with
/// the lock's reason under it, the note, the caption naming the folder settings the home's
/// `config.json` named outside it, the folder Save config writes to with Save as
/// profile… at its end (the lock's reason under it when no picker shows), the Save as profile
/// row while it is open, the listing's caption, and a hairline. `overlays` receives the
/// picker's open list, which must be painted after the scrim of any modal the group sits in.
pub(super) fn group(s: &AppState, k: &Kit, overlays: &mut Vec<El>) -> El {
    let lock = s.data_folder_lock();
    let labels = picker_labels(s);
    let picker_shown = labels.len() > 1;
    let mut group = col()
        .key(GROUP_KEY)
        .role("group")
        .label(PROFILE)
        .gap(SPACE_SM)
        .w_full()
        .child(parts::field_label(k, PROFILE).a11y_hidden());
    if picker_shown {
        group = group.child(picker(s, k, &labels, lock.is_some(), overlays));
        if let Some(reason) = lock {
            group = group.child(parts::caption(k, reason));
        }
    }
    if let Some(note) = s.profile_note() {
        group = group.child(note_line(k, note));
    }
    if let Some(caption) = &s.folders_caption {
        group = group.child(parts::caption(k, caption));
    }
    group = group.child(saves_to(s, k, lock));
    if let (false, Ok(_), Some(reason)) = (picker_shown, s.workspace(), lock) {
        group = group.child(parts::caption(k, reason));
    }
    match s.new_profile_row() {
        NewProfileRow::Open { name_issue, .. } => {
            group = group.child(new_profile_row(s, k, name_issue.as_deref(), lock));
        }
        NewProfileRow::Closed => {}
    }
    if let Some(caption) = list_caption(s) {
        group = group.child(parts::caption(k, &caption));
    }
    group.child(parts::hairline(k).mt(SPACE_SM))
}

/// "Saves to" with the home's `config.json`, and Save as profile… at the row's end while there
/// is a workspace and the Save as profile row is closed: disabled while the data folder is
/// held (the save ends in a folder change), with the reason as its description.
fn saves_to(s: &AppState, k: &Kit, lock: Option<&'static str>) -> El {
    let saves_to = s.home().dir().join(CONFIG_FILE_NAME);
    let mut line = row()
        .gap(SPACE_SM)
        .w_full()
        .items_end()
        .child(parts::detail_row(k, "Saves to", &display_path(&saves_to)).grow(1.0));
    let closed = match s.new_profile_row() {
        NewProfileRow::Closed => true,
        NewProfileRow::Open { .. } => false,
    };
    if closed && s.workspace().is_ok() {
        let mut button = Button::new("Save as profile…")
            .variant(ButtonVariant::Outline)
            .disabled(lock.is_some())
            .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                dispatch(s, Input::NewProfileOpened);
                move_profile_focus(s, tx);
            })
            .build(k)
            .key(NEW_PROFILE_KEY);
        if let Some(reason) = lock {
            button = button.description(reason);
        }
        line = line.child(button);
    }
    line
}

/// The Save as profile row: the profile name with its issue, where the profile will send, the
/// stored secret it gets a copy of, where it keeps its data, the row's issue or confirm line,
/// and Cancel / Save profile (Save anyway while the confirm shows). Both buttons are disabled
/// while the profile saves; Save profile is also disabled while the data folder is held.
fn new_profile_row(
    s: &AppState,
    k: &Kit,
    name_issue: Option<&str>,
    lock: Option<&'static str>,
) -> El {
    let creating = s.save_status.creating().is_some();
    let mut block = col()
        .key(NEW_PROFILE_ROW_KEY)
        .role("group")
        .label("New profile")
        .gap(SPACE_SM)
        .w_full()
        .child(parts::field_label(k, PROFILE_NAME).a11y_hidden())
        .child(
            TextInput::new(&s.new_profile_name)
                .label(PROFILE_NAME)
                .width(None)
                .invalid(name_issue.is_some())
                .on_edit(name_edit)
                .on_change(|s: &mut AppState, _: &mut Tx| {
                    dispatch(s, Input::NewProfileNameEdited);
                })
                .on_submit(submit_name)
                .on_key(name_key)
                .build(k)
                .key(NEW_PROFILE_NAME_KEY),
        );
    if let Some(issue) = name_issue {
        block = block.child(issue_line(k, NAME_ISSUE_KEY, issue));
    }
    block = block.child(destination_line(s, k));
    if let Some(copied) = s.new_profile_secret() {
        block = block.child(copied_line(k, copied));
    }
    let folder = display_line(s.new_profile_data_folder().chars());
    block = block.child(parts::detail_row(k, "Data folder in the profile", &folder));
    let line = s.new_profile_line();
    let confirm = match &line {
        Some(RowLine::Confirm(text)) => {
            block = block.child(status_line(ROW_ISSUE_KEY, text, parts::warning(k, text)));
            Some(text.as_str())
        }
        Some(RowLine::Issue(text)) => {
            block = block.child(issue_line(k, ROW_ISSUE_KEY, text));
            None
        }
        None => None,
    };
    let mut save = Button::new(if confirm.is_some() {
        "Save anyway"
    } else {
        "Save profile"
    })
    .variant(ButtonVariant::Outline)
    .disabled(creating || lock.is_some())
    .on_press_tx(press_create)
    .build(k)
    .key(NEW_PROFILE_SAVE_KEY);
    if let Some(text) = confirm {
        save = save.description(text);
    }
    let actions = row()
        .gap(SPACE_SM)
        .w_full()
        .child(spacer())
        .child(
            Button::new("Cancel")
                .variant(ButtonVariant::Ghost)
                .disabled(creating)
                .on_press_tx(|s: &mut AppState, tx: &mut Tx| {
                    dispatch(s, Input::NewProfileCancelled);
                    move_profile_focus(s, tx);
                })
                .build(k)
                .key(NEW_PROFILE_CANCEL_KEY),
        )
        .child(save);
    parts::reveal(block.child(actions))
}

fn name_edit(s: &mut AppState) -> &mut TextEdit {
    &mut s.new_profile_name
}

/// Save profile, then focus where the press asked for it (the name field, when the name has an
/// issue).
fn press_create(s: &mut AppState, tx: &mut Tx) {
    dispatch(s, Input::CreateProfilePressed);
    move_profile_focus(s, tx);
}

/// Return in the name field, then focus where it asked for it (Save anyway, while the confirm
/// shows).
fn submit_name(s: &mut AppState, tx: &mut Tx) {
    dispatch(s, Input::NewProfileNameSubmitted);
    move_profile_focus(s, tx);
}

/// Escape in the name field closes the row and is taken there, so the sheet's own Escape never
/// closes the sheet with a name typed; while the profile saves it changes nothing.
fn name_key(s: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if key.key != Key::Escape {
        return false;
    }
    dispatch(s, Input::NewProfileCancelled);
    move_profile_focus(s, tx);
    true
}

/// Moves focus to the Save as profile control the state asked for, once.
pub(crate) fn move_profile_focus(s: &mut AppState, tx: &mut Tx) {
    let Some(focus) = s.take_profile_focus() else {
        return;
    };
    let key = match focus {
        ProfileFocus::NameField => NEW_PROFILE_NAME_KEY,
        ProfileFocus::SaveAsButton => NEW_PROFILE_KEY,
        ProfileFocus::SaveProfileButton => NEW_PROFILE_SAVE_KEY,
    };
    let ring = tx.focus_visible();
    tx.focus(key, ring);
}

/// Where the new profile will send, as `namespace/hub`: the form's names, or those the typed
/// connection string gives, said in the label; a string that gives none in the issue color;
/// else that `eh send` asks. Not a live region: it changes as the fields are typed.
fn destination_line(s: &AppState, k: &Kit) -> El {
    match s.new_profile_destination() {
        ProfileDestination::Typed(names) => parts::detail_row(k, EVENT_HUB, &names.display()),
        ProfileDestination::FromString(names) => parts::detail_row(
            k,
            "Event hub (from the connection string)",
            &names.display(),
        ),
        ProfileDestination::Invalid => col()
            .gap(SPACE_XS)
            .w_full()
            .child(parts::caption(k, EVENT_HUB))
            .child(parts::issue(k, INVALID_CONNECTION_STRING)),
        ProfileDestination::None => parts::detail_row(k, EVENT_HUB, NO_DESTINATION),
    }
}

/// The stored secret the new profile gets a copy of from the local store.
fn copied_line(k: &Kit, kind: SecretKind) -> El {
    let label = match kind {
        SecretKind::ConnectionString => "Connection string",
        SecretKind::ClientSecret => "Client secret",
    };
    parts::detail_row(k, label, "Copied from local store")
}

/// An issue in the danger caption, as a polite live region named by the whole line.
fn issue_line(k: &Kit, key: &str, text: &str) -> El {
    status_line(key, text, parts::issue(k, text))
}

/// `visible` showing `text`, as a polite live region keyed `key` and named by the whole line.
fn status_line(key: &str, text: &str, visible: El) -> El {
    col()
        .key(key)
        .role("status")
        .label(text)
        .live_region(LiveRegion::Polite)
        .w_full()
        .child(visible.a11y_hidden())
}

/// "Launch folder", the listed profiles that load, and the current home when it is a profile
/// the listing no longer shows.
fn picker_labels(s: &AppState) -> Vec<String> {
    let mut labels = vec![LAUNCH_FOLDER.to_owned()];
    if let Some(entries) = s.profile_list().entries() {
        labels.extend(
            entries
                .iter()
                .filter(|entry| entry.state.is_ready())
                .map(|entry| display_line(entry.profile.name().chars())),
        );
    }
    let home = home_label(s.home());
    if !labels.contains(&home) {
        labels.push(home);
    }
    labels
}

/// The kit select over `labels`, naming the home a load is heading to, else the current one;
/// `disabled` while the data folder is held.
fn picker(s: &AppState, k: &Kit, labels: &[String], disabled: bool, overlays: &mut Vec<El>) -> El {
    let current = match s.profile_switch() {
        ProfileSwitch::Loading { to, .. } => home_label(to),
        ProfileSwitch::Idle => home_label(s.home()),
    };
    Select::new(PICKER_KEY, labels)
        .label(PROFILE)
        .appearance(SelectAppearance::Field)
        .width(SHEET_FIELD_WIDTH)
        .current(Some(&current))
        .open(s.profile_picker_open && !disabled)
        .disabled(disabled)
        .on_toggle(|s: &mut AppState| s.profile_picker_open = !s.profile_picker_open)
        .on_close(|s: &mut AppState| s.profile_picker_open = false)
        .on_choose(|s: &mut AppState, label: &str, _: &mut Tx| {
            s.profile_picker_open = false;
            match s.home_for_label(label) {
                Some(home) => dispatch(s, Input::ProfilePicked(home)),
                // A listing that lands between the frame that drew this option and the press
                // can drop its profile; the press then picks nothing, and the list shows the
                // new entries.
                None => debug!("the picked profile is no longer listed"),
            }
        })
        .build(k, overlays)
}

/// The note in caption type (a failure in the danger color, cut short with View details), as
/// a polite live region whose name is the whole note.
fn note_line(k: &Kit, note: &ProfileNote) -> El {
    let text = note.text();
    let preview = error_preview(&text);
    let line = if note.is_failure() {
        parts::issue(k, &preview.shown)
    } else {
        parts::caption(k, &preview.shown)
    };
    let mut block = col().gap(SPACE_SM).w_full().child(
        col()
            .key(NOTE_KEY)
            .role("status")
            .label(text)
            .live_region(LiveRegion::Polite)
            .w_full()
            .child(line.a11y_hidden()),
    );
    if preview.cut {
        block = block.child(details::button(k, NOTE_DETAILS_KEY, open_note));
    }
    block
}

/// Opens the whole note; nothing once it is gone.
fn open_note(s: &mut AppState, _: &mut Tx) {
    let Some(text) = s.profile_note().map(ProfileNote::text) else {
        return;
    };
    details::show(s, PROFILE, &text);
}

/// What the listing says: why there is no workspace or no listing, that there are no profiles
/// yet, or how many can't load; nothing while the first listing runs or when all load.
fn list_caption(s: &AppState) -> Option<String> {
    if let Err(reason) = s.workspace() {
        return Some(display_line(reason.chars()));
    }
    let entries = match s.profile_list() {
        ProfileList::Failed(reason) => {
            let line = format!("Profiles could not be listed: {reason}");
            return Some(display_line(line.chars()));
        }
        ProfileList::NotLoaded | ProfileList::Loading { previous: None, .. } => return None,
        ProfileList::Loading {
            previous: Some(entries),
            ..
        }
        | ProfileList::Loaded(entries) => entries,
    };
    let cannot = entries
        .iter()
        .filter(|entry| !entry.state.is_ready())
        .count();
    match cannot {
        0 if entries.is_empty() => Some(NO_PROFILES.to_owned()),
        0 => None,
        1 => Some("1 profile can't load; eh profile list shows why.".to_owned()),
        many => Some(format!(
            "{many} profiles can't load; eh profile list shows why."
        )),
    }
}
