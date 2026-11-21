use super::builders::{
    BILLING, ORDERS, ORDERS_DEV, cannot_load, launch_folder, orders_config, profile_home, state,
    with_profiles,
};
use super::journeys::{open_listed_sheet, pick_with_pointer};
use super::services::ServiceCall;
use super::{Harness, a11y_node, lists};
use crate::design::tokens::{WINDOW_MIN, WINDOW_SIZE};
use crate::state::{AppState, Input, NO_PROFILES};
use crate::views::{PICKER_KEY, PROFILE_GROUP_KEY, PROFILE_NOTE_KEY};
use rune::Key;
use rune::accesskit::Action;
use shared_eventhub_persistence::config::{CONFIG_FILE_NAME, ConfigLoad};
use shared_eventhub_persistence::workspace::ProfileEntry;

const TRIGGER: &str = "profile-picker-trigger";
const LOCKED: &str = "The data folder can change once the cancelled connection stops.";

fn orders_form() -> AppState {
    let mut s = with_profiles(state());
    s.form.connection_string.edit_mut().replace_text(ORDERS);
    s
}

/// The sheet open over the listed test workspace, with a typed connection string.
pub(super) fn profiles_sheet(size: (f32, f32)) -> Harness {
    let mut h = Harness::new(orders_form(), size);
    open_listed_sheet(&mut h);
    h
}

/// [`profiles_sheet`] with the picker's list open.
pub(super) fn picker_open(size: (f32, f32)) -> Harness {
    let mut h = profiles_sheet(size);
    h.press(TRIGGER);
    h
}

/// The sheet over the listed workspace while a connect cancelled while preparing holds the
/// data folder.
pub(super) fn picker_locked(size: (f32, f32)) -> Harness {
    let mut h = Harness::new(orders_form(), size);
    h.deliver(Input::ConnectPressed);
    h.deliver(Input::DisconnectPressed);
    h.take_calls();
    open_listed_sheet(&mut h);
    h
}

/// The sheet after a pick of `billing` whose load failed with a long reason.
pub(super) fn load_failed_note(size: (f32, f32)) -> Harness {
    let mut h = profiles_sheet(size);
    let request = pick_with_pointer(&mut h, BILLING);
    h.deliver(Input::ProfileLoaded {
        request,
        result: Err(format!(
            "'{}' is an eh config, but it is not valid: {}",
            profile_home(BILLING).dir().join(CONFIG_FILE_NAME).display(),
            "unknown field `<redacted>`, expected one of `tool`, `schema_version`, `target`, \
             `capture`, `storage`"
        )),
    });
    h
}

fn focus(h: &mut Harness, key: &str) {
    assert!(
        h.rt.accessibility_action(key, Action::Focus, None),
        "nothing keyed {key} takes focus"
    );
    h.settle();
}

#[test]
fn a_profile_picked_with_the_keyboard_takes_the_saves_and_the_connect() {
    let mut h = profiles_sheet(WINDOW_SIZE);
    focus(&mut h, TRIGGER);

    h.key(Key::Text(" ".into()));
    h.key(Key::Down);
    h.key(Key::Down);
    h.key(Key::Text(" ".into()));
    let request = match h.take_calls().as_slice() {
        [ServiceCall::LoadProfile { request, dir }] => {
            assert_eq!(dir, profile_home(ORDERS_DEV).dir());
            *request
        }
        other => panic!("expected one LoadProfile, got {other:?}"),
    };
    h.deliver(Input::ProfileLoaded {
        request,
        result: Ok(ConfigLoad::Loaded(Box::new(orders_config()))),
    });
    let saves_to = profile_home(ORDERS_DEV).dir().join(CONFIG_FILE_NAME);
    let shows_saves_to = h.has_text(&saves_to.display().to_string());
    h.press("sheet-connect");
    let prepared = h
        .calls
        .take_where(|call| matches!(call, ServiceCall::PrepareSession { .. }));
    h.press("pill");

    assert!(
        shows_saves_to,
        "the Saves to row names the profile's config.json"
    );
    let [ServiceCall::PrepareSession { request, .. }] = prepared.as_slice() else {
        panic!("expected one PrepareSession, got {prepared:?}");
    };
    assert!(
        request
            .paths
            .base()
            .starts_with(profile_home(ORDERS_DEV).dir()),
        "{:?}",
        request.paths
    );
    assert_eq!(
        h.label_of("pill").as_deref(),
        Some("Connection: Preparing, profile orders-dev")
    );
    assert!(h.has_text(ORDERS_DEV), "the popover's Profile row names it");
}

#[test]
fn the_profile_group_is_a_named_group_with_a_named_picker() {
    let h = profiles_sheet(WINDOW_SIZE);

    let tree = h.a11y();

    let group = a11y_node(&tree, PROFILE_GROUP_KEY).unwrap();
    let picker = a11y_node(&tree, TRIGGER).unwrap();
    assert_eq!(group["role"], "group");
    assert_eq!(group["name"], "Profile");
    assert_eq!(picker["role"], "combo-box");
    assert_eq!(picker["name"], "Profile");
    assert!(lists(picker, "actions", "click"), "{picker}");
}

#[test]
fn the_picker_lists_the_launch_folder_and_the_profiles_that_load() {
    let h = picker_open(WINDOW_SIZE);

    let content = h.path_of(&format!("{PICKER_KEY}-content"));
    let options: Vec<String> =
        h.rt.elements()
            .into_iter()
            .filter(|element| element.role == "option" && element.path.starts_with(&content))
            .filter_map(|element| element.label)
            .collect();

    assert_eq!(options, ["Launch folder", BILLING, ORDERS_DEV]);
    assert!(h.has_text("1 profile can't load; eh profile list shows why."));
}

#[test]
fn under_the_lock_the_picker_reads_as_disabled_with_the_reason_under_it() {
    let h = picker_locked(WINDOW_SIZE);

    let tree = h.a11y();
    let picker = a11y_node(&tree, TRIGGER).unwrap();

    assert!(lists(picker, "states", "disabled"), "{picker}");
    assert!(!lists(picker, "actions", "click"), "{picker}");
    assert!(h.has_text(LOCKED));
}

#[test]
fn the_note_is_a_polite_live_region_carrying_the_whole_note() {
    let h = load_failed_note(WINDOW_SIZE);

    let tree = h.a11y();
    let note = a11y_node(&tree, PROFILE_NOTE_KEY).unwrap();

    assert_eq!(note["role"], "status");
    assert!(lists(note, "states", "live-polite"), "{note}");
    let name = note["name"].as_str().unwrap_or_default();
    assert!(
        name.starts_with("Couldn't open profile billing: ")
            && name.ends_with("Pick another profile or fix its config.json."),
        "{name}"
    );
    assert!(
        h.has_key("profile-note-details"),
        "a cut note offers its details"
    );
}

#[test]
fn a_cut_note_opens_whole_in_the_details() {
    let mut h = load_failed_note(WINDOW_SIZE);

    h.press("profile-note-details");

    let shown = h.state().details().map(|details| details.text.clone());
    assert!(
        shown.is_some_and(|text| text.ends_with("Pick another profile or fix its config.json.")),
        "the details hold the whole note"
    );
}

#[rstest::rstest]
#[case::smallest(WINDOW_MIN)]
#[case::default_size(WINDOW_SIZE)]
fn the_open_list_draws_over_the_sheet_inside_the_window(#[case] size: (f32, f32)) {
    let h = picker_open(size);

    let list =
        h.rt.elements()
            .into_iter()
            .find(|element| {
                element.key.as_deref() == Some(&format!("{PICKER_KEY}-content")) && !element.exiting
            })
            .unwrap();
    let [x, y, w, height] = list.rect;
    let [cx, cy] = [x + w / 2.0, y + height / 2.0];

    assert!(x >= 0.0 && y >= 0.0 && x + w <= size.0 && y + height <= size.1);
    let picked =
        h.rt.element_at(cx, cy)
            .map(|picked| picked.path().to_owned());
    assert!(
        picked
            .as_deref()
            .is_some_and(|path| path.starts_with(&list.path)),
        "the list is under something: {picked:?}"
    );
}

/// The sheet opened over a workspace that lists as `listed` (`None`: no workspace at all).
fn sheet_listing(listed: Option<Result<Vec<ProfileEntry>, String>>) -> Harness {
    let mut s = orders_form();
    if listed.is_none() {
        s.set_launch(launch_folder(), Err("No home folder was found".to_owned()));
    }
    let mut h = Harness::new(s, WINDOW_SIZE);
    h.press("connect");
    let request = h.take_calls().into_iter().find_map(|call| match call {
        ServiceCall::ListProfiles { request, .. } => Some(request),
        _ => None,
    });
    if let (Some(request), Some(result)) = (request, listed) {
        h.deliver(Input::ProfilesListed { request, result });
    }
    h
}

#[rstest::rstest]
#[case::no_workspace(None, "No home folder was found")]
#[case::no_profiles(Some(Ok(Vec::new())), NO_PROFILES)]
#[case::listing_failed(
    Some(Err("Failed to list the workspace".to_owned())),
    "Profiles could not be listed: Failed to list the workspace"
)]
#[case::two_cannot_load(
    Some(Ok(vec![cannot_load("one"), cannot_load("two")])),
    "2 profiles can't load; eh profile list shows why."
)]
fn the_listing_caption_says_what_the_workspace_holds(
    #[case] listed: Option<Result<Vec<ProfileEntry>, String>>,
    #[case] caption: &str,
) {
    let h = sheet_listing(listed);

    assert!(h.has_text(caption), "no caption {caption:?}");
    assert!(!h.has_key(TRIGGER), "a picker with only the launch folder");
}

#[test]
fn a_home_the_listing_no_longer_shows_stays_in_the_picker() {
    let mut h = profiles_sheet(WINDOW_SIZE);
    let request = pick_with_pointer(&mut h, ORDERS_DEV);
    h.deliver(Input::ProfileLoaded {
        request,
        result: Ok(ConfigLoad::Loaded(Box::new(orders_config()))),
    });
    h.deliver(Input::CloseSheet);
    h.take_calls();
    h.press("connect");
    let request = match h.take_calls().as_slice() {
        [ServiceCall::ListProfiles { request, .. }] => *request,
        other => panic!("expected one ListProfiles, got {other:?}"),
    };
    h.deliver(Input::ProfilesListed {
        request,
        result: Ok(Vec::new()),
    });

    h.press(TRIGGER);

    let content = h.path_of(&format!("{PICKER_KEY}-content"));
    let options: Vec<String> =
        h.rt.elements()
            .into_iter()
            .filter(|element| element.role == "option" && element.path.starts_with(&content))
            .filter_map(|element| element.label)
            .collect();
    assert_eq!(options, ["Launch folder", ORDERS_DEV]);
}

#[test]
fn choosing_the_current_home_starts_no_load() {
    let mut h = profiles_sheet(WINDOW_SIZE);

    h.press(TRIGGER);
    h.press_named(&format!("{PICKER_KEY}-content"), "Launch folder");

    assert!(h.take_calls().is_empty());
    assert!(!h.state().profile_picker_open);
}
