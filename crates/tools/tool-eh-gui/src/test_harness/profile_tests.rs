use super::builders::{
    BILLING, ORDERS, ORDERS_DEV, cannot_load, launch_folder, orders_config, profile, profile_home,
    state, with_profiles,
};
use super::journeys::{open_listed_sheet, pick_with_pointer};
use super::services::ServiceCall;
use super::{Harness, a11y_node, a11y_nodes, element, focused_key, lists};
use crate::design::tokens::{TYPE_CAPTION, WINDOW_MIN, WINDOW_SIZE};
use crate::services::CreateRequest;
use crate::state::{AppState, Input, NO_PROFILES, NewProfileRow, RECALLED_FIELD, recalled};
use crate::views::{
    NAME_ISSUE_KEY, NEW_PROFILE_CANCEL_KEY, NEW_PROFILE_KEY, NEW_PROFILE_NAME_KEY,
    NEW_PROFILE_ROW_KEY, NEW_PROFILE_SAVE_KEY, PICKER_KEY, PROFILE_GROUP_KEY, PROFILE_NOTE_KEY,
    ROW_ISSUE_KEY, SAVE_STATUS_KEY, SAVE_STATUS_LINES,
};
use rune::accesskit::Action;
use rune::text::{RichLayoutOptions, measure_with};
use rune::{Key, KeyEvent, Mods};
use shared_eventhub_persistence::config::{
    AuthMethodConfig, CONFIG_FILE_NAME, ConfigLoad, CredentialKindConfig, StoredSecret,
    TargetConfig, ToolConfig,
};
use shared_eventhub_persistence::export::DiskWriteMode;
use shared_eventhub_persistence::workspace::{ConfigHome, ProfileEntry};

const TRIGGER: &str = "profile-picker-trigger";
const LOCKED: &str = "The data folder can change once the cancelled connection stops.";
const NEW: &str = "orders-new";

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
fn the_sheet_shows_the_caption_for_folders_the_launch_config_named_outside_it() {
    let caption = "config.json names a data folder outside this folder; it was not used.";
    let mut s = orders_form();
    s.folders_caption = Some(caption.to_owned());
    let mut h = Harness::new(s, WINDOW_SIZE);

    open_listed_sheet(&mut h);

    assert!(h.has_text(caption));
}

#[rstest::rstest]
#[case::smallest(WINDOW_MIN)]
#[case::default_size(WINDOW_SIZE)]
fn the_footer_fits_the_whole_refusal_of_a_received_message_path_outside_the_launch_folder(
    #[case] size: (f32, f32),
) {
    let mut s = orders_form();
    s.form.disk_write = DiskWriteMode::Immediate;
    s.form.received_msg_path.replace_text("../outside");
    let mut h = Harness::new(s, size);
    open_listed_sheet(&mut h);

    h.deliver(Input::SavePressed);

    let status = element(&h.rt, SAVE_STATUS_KEY);
    let [_, _, width, height] = status.rect;
    let text = status.text.unwrap_or_default();
    let (_, needed) = measure_with(
        &text,
        &[],
        TYPE_CAPTION,
        Some(width),
        RichLayoutOptions::new(),
    );
    assert_eq!(
        text,
        "Not saved: The received message path is outside this folder; config.json here keeps \
         only folders inside. Pick one inside or use a profile."
    );
    assert!(
        needed <= SAVE_STATUS_LINES as f32 * TYPE_CAPTION.line_px() + 0.5,
        "the text needs {needed} pt at {width} pt wide"
    );
    assert!(
        height >= needed - 0.5,
        "the caption is {height} pt tall and its text needs {needed} pt"
    );
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

/// The sheet open over a workspace with no profiles yet, with a typed connection string.
fn empty_workspace_sheet() -> Harness {
    let mut h = Harness::new(orders_form(), WINDOW_SIZE);
    h.press("connect");
    let request = match h.take_calls().as_slice() {
        [ServiceCall::ListProfiles { request, .. }] => *request,
        other => panic!("expected one ListProfiles, got {other:?}"),
    };
    h.deliver(Input::ProfilesListed {
        request,
        result: Ok(Vec::new()),
    });
    h
}

/// The only call recorded so far, which must be a create.
fn only_create(h: &mut Harness) -> CreateRequest {
    let mut calls = h.take_calls();
    assert_eq!(calls.len(), 1, "expected one call, got {calls:?}");
    let Some(ServiceCall::CreateProfile(request)) = calls.pop() else {
        panic!("the call was not a CreateProfile");
    };
    request
}

/// The texts drawn inside the element keyed `key`.
fn texts_within(h: &Harness, key: &str) -> Vec<String> {
    let inside = format!("{}/", h.path_of(key));
    h.rt.elements()
        .into_iter()
        .filter(|element| element.path.starts_with(&inside) && !element.exiting)
        .filter_map(|element| element.text)
        .collect()
}

fn row_open(h: &Harness) -> bool {
    match h.state().new_profile_row() {
        NewProfileRow::Open { .. } => true,
        NewProfileRow::Closed => false,
    }
}

#[test]
fn a_profile_saved_with_return_opens_and_focus_comes_back_to_save_as_profile() {
    let mut h = empty_workspace_sheet();

    h.press(NEW_PROFILE_KEY);
    let field_focused = h.state().new_profile_name.focused;
    h.rt.paste(NEW);
    h.settle();
    h.key(Key::Enter);
    let request = only_create(&mut h);
    h.deliver(Input::ProfileCreated {
        request: request.request,
        result: Ok(profile(NEW)),
    });
    let focus_after_save = focused_key(&h.rt);
    let load = match h.take_calls().as_slice() {
        [
            ServiceCall::ListProfiles { .. },
            ServiceCall::LoadProfile { request, dir },
        ] => {
            assert_eq!(dir, profile(NEW).dir());
            *request
        }
        other => panic!("expected a ListProfiles and a LoadProfile, got {other:?}"),
    };
    h.deliver(Input::ProfileLoaded {
        request: load,
        result: Ok(ConfigLoad::Loaded(Box::new(request.config))),
    });

    assert!(field_focused, "Save as profile… focuses the name field");
    assert_eq!(request.name.as_str(), NEW);
    assert_eq!(focus_after_save.as_deref(), Some(NEW_PROFILE_KEY));
    assert_eq!(h.state().home(), &ConfigHome::Profile(profile(NEW)));
    assert!(texts_within(&h, TRIGGER).iter().any(|text| text == NEW));
    let saves_to = profile(NEW).dir().join(CONFIG_FILE_NAME);
    assert!(h.has_text(&saves_to.display().to_string()));
}

#[test]
fn with_no_picker_the_lock_reason_shows_once_and_save_as_profile_is_disabled() {
    let mut h = Harness::new(orders_form(), WINDOW_SIZE);
    h.deliver(Input::ConnectPressed);
    h.deliver(Input::DisconnectPressed);
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

    let tree = h.a11y();
    let button = a11y_node(&tree, NEW_PROFILE_KEY).unwrap();
    let shown = texts_within(&h, PROFILE_GROUP_KEY)
        .iter()
        .filter(|text| text.as_str() == LOCKED)
        .count();

    assert!(!h.has_key(TRIGGER), "no picker over an empty workspace");
    assert!(lists(button, "states", "disabled"), "{button}");
    assert_eq!(shown, 1, "the Profile group names the lock once");
}

#[test]
fn escape_in_the_name_field_closes_the_row_and_keeps_the_sheet() {
    let mut h = empty_workspace_sheet();
    h.press(NEW_PROFILE_KEY);
    h.rt.paste(NEW);
    h.settle();

    h.key(Key::Escape);

    assert!(!row_open(&h));
    assert!(h.state().sheet.is_some());
    assert_eq!(focused_key(&h.rt).as_deref(), Some(NEW_PROFILE_KEY));
}

#[test]
fn a_second_escape_closes_the_sheet() {
    let mut h = empty_workspace_sheet();
    h.press(NEW_PROFILE_KEY);
    h.key(Key::Escape);

    h.key(Key::Escape);

    assert!(h.state().sheet.is_none());
}

#[test]
fn tab_walks_from_the_picker_to_save_as_profile_and_through_the_open_row() {
    let mut h = profiles_sheet(WINDOW_SIZE);
    focus(&mut h, TRIGGER);
    h.key(Key::Tab);
    let after_picker = focused_key(&h.rt);
    h.press(NEW_PROFILE_KEY);

    h.key(Key::Tab);
    let after_field = focused_key(&h.rt);
    h.key(Key::Tab);
    let after_cancel = focused_key(&h.rt);

    assert_eq!(after_picker.as_deref(), Some(NEW_PROFILE_KEY));
    assert_eq!(after_field.as_deref(), Some(NEW_PROFILE_CANCEL_KEY));
    assert_eq!(after_cancel.as_deref(), Some(NEW_PROFILE_SAVE_KEY));
}

#[test]
fn a_refused_name_focuses_the_field_again() {
    let mut h = empty_workspace_sheet();
    h.press(NEW_PROFILE_KEY);

    h.press(NEW_PROFILE_SAVE_KEY);

    assert!(h.state().new_profile_name.focused);
    assert!(h.has_key(NAME_ISSUE_KEY));
    assert!(h.take_calls().is_empty());
}

/// What the launch folder's `config.json` holds: Entra ID for the `orders` hub.
fn named_config() -> ToolConfig {
    ToolConfig::new(TargetConfig {
        namespace: Some("orders.servicebus.windows.net".to_owned()),
        event_hub: Some("orders".to_owned()),
        consumer_group: "$Default".to_owned(),
        auth: AuthMethodConfig::Credential {
            credential: CredentialKindConfig::DeveloperTools,
        },
    })
}

/// The sheet over the listed workspace at `size`, launched in a folder whose `config.json` is
/// `config`.
fn recalled_from(config: ToolConfig, size: (f32, f32)) -> Harness {
    let mut s = with_profiles(state());
    s.form = recalled(
        &ConfigHome::CurrentFolder(launch_folder()),
        Ok(ConfigLoad::Loaded(Box::new(config))),
    )
    .form;
    let mut h = Harness::new(s, size);
    open_listed_sheet(&mut h);
    h
}

/// The sheet launched in a folder whose `config.json` names a hub.
fn recalled_sheet() -> Harness {
    recalled_from(named_config(), WINDOW_SIZE)
}

/// What the launch folder's `config.json` holds: a stored connection string for the `orders`
/// hub, named in full.
fn named_string_config() -> ToolConfig {
    ToolConfig::new(TargetConfig {
        auth: AuthMethodConfig::ConnectionString {
            stored: StoredSecret::LocalDb,
        },
        ..named_config().target
    })
}

const CONFIRM: &str = "The namespace and event hub came from the launch folder's config.json. A \
                       profile is trusted by eh send without asking.";

fn shift(h: &mut Harness, key: Key) {
    h.rt.key(KeyEvent {
        key,
        mods: Mods {
            shift: true,
            ..Mods::default()
        },
    });
    h.settle();
}

#[test]
fn recalled_names_survive_focus_caret_moves_a_method_switch_a_save_and_a_reopen() {
    let mut h = recalled_sheet();

    h.press_named("connect-form", "Namespace");
    h.key(Key::End);
    h.key(Key::Home);
    h.key(Key::Right);
    shift(&mut h, Key::End);
    h.key(Key::Tab);
    h.press_named("connect-form", "Connection string");
    h.type_into("connection-string", ORDERS);
    h.press("save-config");
    h.deliver(Input::ConfigSaved(Ok(())));
    h.take_calls();
    h.deliver(Input::CloseSheet);
    open_listed_sheet(&mut h);

    let form = &h.state().form;
    assert!(form.connection_string.is_stored(), "the save settled");
    assert_eq!(form.namespace.text, "orders.servicebus.windows.net");
    assert!(form.recalled_fields().namespace && form.recalled_fields().event_hub);
}

#[derive(Clone, Copy, Debug)]
enum Edit {
    Typed,
    Deleted,
    Pasted,
    Undone,
}

#[rstest::rstest]
#[case::typed(Edit::Typed)]
#[case::deleted(Edit::Deleted)]
#[case::pasted(Edit::Pasted)]
#[case::undone(Edit::Undone)]
fn an_edit_of_a_recalled_name_makes_it_typed(#[case] edit: Edit) {
    let mut h = recalled_sheet();
    h.press_named("connect-form", "Namespace");

    match edit {
        Edit::Typed => h.key(Key::Text("x".into())),
        Edit::Deleted => h.key(Key::Backspace),
        Edit::Pasted => {
            h.rt.paste("x");
            h.settle();
        }
        Edit::Undone => {
            h.key(Key::Text("x".into()));
            h.command(Key::Command('z'));
        }
    }

    let recalled = h.state().form.recalled_fields();
    assert!(!recalled.namespace, "{edit:?} left the namespace recalled");
    assert!(recalled.event_hub);
}

#[test]
fn under_the_lock_save_as_profile_reads_as_disabled_with_the_reason() {
    let h = picker_locked(WINDOW_SIZE);

    let tree = h.a11y();
    let button = a11y_node(&tree, NEW_PROFILE_KEY).unwrap();

    assert!(lists(button, "states", "disabled"), "{button}");
    assert!(!lists(button, "actions", "click"), "{button}");
    assert_eq!(button["description"], LOCKED);
}

/// The sheet with the Save as profile row open and both lines showing: a press with names
/// recalled from the launch folder asked to confirm them, then a name against the rules was
/// refused.
pub(super) fn new_profile_issues(size: (f32, f32)) -> Harness {
    let mut h = recalled_from(named_string_config(), size);
    h.press(NEW_PROFILE_KEY);
    h.type_into(NEW_PROFILE_NAME_KEY, NEW);
    h.press(NEW_PROFILE_SAVE_KEY);
    h.type_into(NEW_PROFILE_NAME_KEY, "orders new");
    h.press(NEW_PROFILE_SAVE_KEY);
    h
}

#[test]
fn the_row_is_a_named_group_and_its_lines_are_polite_live_regions() {
    let h = new_profile_issues(WINDOW_SIZE);

    let tree = h.a11y();

    let group = a11y_node(&tree, NEW_PROFILE_ROW_KEY).unwrap();
    assert_eq!(group["role"], "group");
    assert_eq!(group["name"], "New profile");
    let named_field = a11y_nodes(&tree)
        .into_iter()
        .filter(|node| node["name"] == "Profile name")
        .count();
    assert_eq!(named_field, 1, "the field's name is read once");
    let name_issue = a11y_node(&tree, NAME_ISSUE_KEY).unwrap();
    let row_issue = a11y_node(&tree, ROW_ISSUE_KEY).unwrap();
    for issue in [name_issue, row_issue] {
        assert_eq!(issue["role"], "status");
        assert!(lists(issue, "states", "live-polite"), "{issue}");
    }
    let name = name_issue["name"].as_str().unwrap_or_default();
    assert!(name.starts_with("A profile name uses letters"), "{name}");
    assert_eq!(row_issue["name"], CONFIRM);
}

/// The calls recorded so far that are creates.
fn creates(h: &mut Harness) -> Vec<ServiceCall> {
    h.calls
        .take_where(|call| matches!(call, ServiceCall::CreateProfile(_)))
}

#[test]
fn recalled_names_are_marked_when_the_row_opens_and_save_anyway_saves_them() {
    let mut h = recalled_from(named_string_config(), WINDOW_SIZE);

    h.press(NEW_PROFILE_KEY);
    let marks = texts_within(&h, "connect-form")
        .iter()
        .filter(|text| text.as_str() == RECALLED_FIELD)
        .count();
    let copied = h.has_text("Copied from local store");
    h.type_into(NEW_PROFILE_NAME_KEY, NEW);
    h.press(NEW_PROFILE_SAVE_KEY);
    let asked = creates(&mut h);
    let tree = h.a11y();
    let save = a11y_node(&tree, NEW_PROFILE_SAVE_KEY).unwrap().clone();
    let confirm = a11y_node(&tree, ROW_ISSUE_KEY).unwrap().clone();
    h.press(NEW_PROFILE_SAVE_KEY);

    assert_eq!(marks, 2, "both recalled names carry a mark");
    assert!(copied, "the row says the stored string is copied");
    assert!(asked.is_empty(), "the first press only asks");
    assert_eq!(save["name"], "Save anyway");
    assert_eq!(save["description"], CONFIRM);
    assert_eq!(confirm["role"], "status");
    assert!(lists(&confirm, "states", "live-polite"), "{confirm}");
    assert_eq!(confirm["name"], CONFIRM);
    let request = only_create(&mut h);
    assert_eq!(
        request.config.target.event_hub.as_deref(),
        Some("orders"),
        "the recalled names are saved as shown"
    );
    assert_eq!(
        request.stored_from,
        Some(h.state().data_paths()),
        "the stored string is copied from the launch folder's data folder"
    );
}

/// [`recalled_from`] the string config with the row open and [`NEW`] typed into its name.
fn confirm_by_keyboard() -> Harness {
    let mut h = recalled_from(named_string_config(), WINDOW_SIZE);
    h.press(NEW_PROFILE_KEY);
    h.rt.paste(NEW);
    h.settle();
    h
}

#[test]
fn two_returns_in_the_name_field_leave_the_confirm_up_and_focus_on_save_anyway() {
    let mut h = confirm_by_keyboard();

    h.key(Key::Enter);
    let field_kept_focus = h.state().new_profile_name.focused;
    h.key(Key::Enter);

    assert!(
        field_kept_focus,
        "focus stays in the name field when the confirm appears"
    );
    assert!(creates(&mut h).is_empty());
    assert_eq!(h.label_of(ROW_ISSUE_KEY).as_deref(), Some(CONFIRM));
    assert_eq!(focused_key(&h.rt).as_deref(), Some(NEW_PROFILE_SAVE_KEY));
}

#[test]
fn a_held_return_in_the_name_field_saves_nothing() {
    let mut h = confirm_by_keyboard();
    let enter = KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    };
    h.key(Key::Enter);

    h.key(Key::Enter);
    for _ in 0..5 {
        h.rt.repeat_key(enter.clone());
        h.settle();
    }

    assert_eq!(focused_key(&h.rt).as_deref(), Some(NEW_PROFILE_SAVE_KEY));
    assert!(creates(&mut h).is_empty());
    assert_eq!(h.label_of(ROW_ISSUE_KEY).as_deref(), Some(CONFIRM));
}
