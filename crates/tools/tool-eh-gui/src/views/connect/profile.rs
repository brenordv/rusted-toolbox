//! The connection sheet's Profile group: which `config.json` the sheet edits, where Save config
//! writes, what the last pick left to say, and the profiles that can't load.

use crate::app::dispatch;
use crate::design::parts;
use crate::design::tokens::{SHEET_FIELD_WIDTH, SPACE_SM};
use crate::read::rows::{display_line, display_path};
use crate::state::{
    AppState, Input, LAUNCH_FOLDER, NO_PROFILES, ProfileList, ProfileNote, ProfileSwitch,
    error_preview, home_label,
};
use crate::views::details;
use rune::{El, LiveRegion, Tx, col};
use rune_kit::{Kit, Select, SelectAppearance};
use shared_eventhub_persistence::config::CONFIG_FILE_NAME;
use tracing::debug;

/// The group's key; its accessible name is "Profile".
pub const GROUP_KEY: &str = "profile-group";
/// The picker's key; its trigger is keyed `{PICKER_KEY}-trigger`.
pub const PICKER_KEY: &str = "profile-picker";
/// The note line's key: a polite live region.
pub const NOTE_KEY: &str = "profile-note";
const NOTE_DETAILS_KEY: &str = "profile-note-details";
const PROFILE: &str = "Profile";

/// The group: its label, the picker (when there is more than the launch folder to pick) with
/// the lock's reason under it, the note, the folder Save config writes to, the listing's
/// caption, and a hairline. `overlays` receives the picker's open list, which must be painted
/// after the scrim of any modal the group sits in.
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
    let saves_to = s.home().dir().join(CONFIG_FILE_NAME);
    group = group.child(parts::detail_row(k, "Saves to", &display_path(&saves_to)));
    if let Some(caption) = list_caption(s) {
        group = group.child(parts::caption(k, &caption));
    }
    group.child(parts::hairline(k).mt(SPACE_SM))
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
