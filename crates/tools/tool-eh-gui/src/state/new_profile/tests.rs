use super::*;
use crate::archive::bulk::BulkExport;
use crate::bridge::test_support::detached_session;
use crate::form::{AuthChoice, CredentialChoice, SecretSource};
use crate::read::inspector::ExportPhase;
use crate::send::Destination;
use crate::send::run::SendRun;
use crate::services::SaveRequest;
use crate::state::{ConnectionState, Input, SheetPurpose, recalled};
use crate::test_harness::builders::{
    BILLING, ORDERS, ORDERS_DEV, billing_config, launch_folder, listing, orders_config, profile,
    profile_home, ready, state, test_workspace, with_profiles,
};
use chrono::Utc;
use rune::TextEdit;
use shared_eventhub_core::filters::{CaptureFilters, TextFilter};
use shared_eventhub_core::models::MessageKey;
use shared_eventhub_core::producer::Pacing;
use shared_eventhub_persistence::config::{
    AuthMethodConfig, ConfigLoad, CredentialKindConfig, StoredSecret, TargetConfig, ToolConfig,
};
use shared_eventhub_persistence::paths::DataPaths;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

const NEW: &str = "orders-new";
const SAVE_LOCK: &str = "The data folder can change once the save finishes.";
const BULK_LOCK: &str = "The data folder can change once the bulk export ends.";

fn loaded(config: ToolConfig) -> Result<ConfigLoad, String> {
    Ok(ConfigLoad::Loaded(Box::new(config)))
}

/// The one effect of `out` that `pick` takes.
fn only<'a, T>(out: &'a Outcome, pick: impl Fn(&'a Effect) -> Option<T>) -> T {
    let found: Vec<T> = out.effects.iter().filter_map(pick).collect();
    let Ok([one]) = <[T; 1]>::try_from(found) else {
        panic!("expected exactly one such effect in {:?}", out.effects);
    };
    one
}

fn create_request(out: &Outcome) -> &CreateRequest {
    only(out, |effect| match effect {
        Effect::CreateProfile(request) => Some(request),
        _ => None,
    })
}

fn load_request(out: &Outcome) -> (u64, PathBuf) {
    only(out, |effect| match effect {
        Effect::LoadProfile { request, dir } => Some((*request, dir.clone())),
        _ => None,
    })
}

fn list_request(out: &Outcome) -> u64 {
    only(out, |effect| match effect {
        Effect::ListProfiles { request, .. } => Some(*request),
        _ => None,
    })
}

fn creates(out: &Outcome) -> bool {
    out.effects
        .iter()
        .any(|effect| matches!(effect, Effect::CreateProfile(_)))
}

/// The sheet open over the listed test workspace with a typed connection string.
fn listed() -> AppState {
    let mut s = with_profiles(state());
    s.form.connection_string.edit_mut().replace_text(ORDERS);
    let out = s.apply(Input::OpenSheet(SheetPurpose::Connect));
    let request = list_request(&out);
    s.apply(Input::ProfilesListed {
        request,
        result: Ok(listing()),
    });
    s
}

/// [`listed`] with the Save as profile row open, its focus request taken as the view does.
fn open_row() -> AppState {
    let mut s = listed();
    s.apply(Input::NewProfileOpened);
    s.take_profile_focus();
    s
}

/// A name typed into the row the way the user types it.
fn typed(mut s: AppState, name: &str) -> AppState {
    s.new_profile_name.insert(name);
    s
}

/// The row open with [`NEW`] typed and Save profile pressed; returns the create's request id.
fn creating() -> (AppState, u64) {
    let mut s = typed(open_row(), NEW);
    let out = s.apply(Input::CreateProfilePressed);
    let request = create_request(&out).request;
    (s, request)
}

fn name_issue(s: &AppState) -> Option<&str> {
    match s.new_profile_row() {
        NewProfileRow::Open { name_issue, .. } => name_issue.as_deref(),
        NewProfileRow::Closed => None,
    }
}

fn row_issue(s: &AppState) -> Option<RowIssue> {
    match s.new_profile_row() {
        NewProfileRow::Open { row_issue, .. } => *row_issue,
        NewProfileRow::Closed => None,
    }
}

fn is_open(s: &AppState) -> bool {
    match s.new_profile_row() {
        NewProfileRow::Open { .. } => true,
        NewProfileRow::Closed => false,
    }
}

/// A config the launch folder's `config.json` holds: Entra ID for the `orders` hub.
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

/// [`open_row`] with the form the launch folder's [`named_config`] gave.
fn recalled_row() -> AppState {
    let mut s = open_row();
    s.form = recalled(
        &ConfigHome::CurrentFolder(launch_folder()),
        loaded(named_config()),
    )
    .form;
    typed(s, NEW)
}

#[test]
fn opening_the_row_empties_the_name_and_asks_for_focus_in_it() {
    let mut s = listed();
    s.new_profile_name = TextEdit::new("left over");

    s.apply(Input::NewProfileOpened);

    assert_eq!(
        s.new_profile_row(),
        &NewProfileRow::Open {
            name_issue: None,
            row_issue: None
        }
    );
    assert!(s.new_profile_name.text.is_empty());
    assert_eq!(s.take_profile_focus(), Some(ProfileFocus::NameField));
}

#[rstest::rstest]
#[case::kept_relative("captures", "captures")]
#[case::default_folder(".eh-data", ".eh-data")]
fn a_valid_press_saves_the_form_with_its_rules_into_the_named_profile(
    #[case] folder: &str,
    #[case] saved: &str,
) {
    let mut s = typed(open_row(), NEW);
    s.form.data_folder = folder.to_owned();
    let rules = CaptureFilters {
        time: None,
        text: Some(TextFilter::Contains(vec!["refund".to_owned()])),
    };
    s.read.rules_applied = rules.clone();

    let out = s.apply(Input::CreateProfilePressed);

    let request = create_request(&out);
    assert_eq!(request.name.as_str(), NEW);
    assert_eq!(request.workspace, test_workspace());
    assert_eq!(request.config.storage.base_data_folder, saved);
    assert_eq!(request.config.capture.filters, rules);
    assert!(request.given.connection_string.is_some());
    assert_eq!(s.save_status.creating().map(ProfileName::as_str), Some(NEW));
    assert_eq!(s.data_folder_lock(), Some(SAVE_LOCK));
    assert_eq!(
        s.new_profile_row(),
        &NewProfileRow::Open {
            name_issue: None,
            row_issue: None
        }
    );
}

#[test]
fn after_change_picked_an_absolute_data_folder_the_profile_uses_the_default() {
    let mut s = open_row();
    s.apply(Input::DataFolderPicked(launch_folder().join("picked")));
    s.form.connection_string.edit_mut().replace_text(ORDERS);
    let mut s = typed(s, NEW);

    let out = s.apply(Input::CreateProfilePressed);

    assert_eq!(s.new_profile_data_folder(), DEFAULT_DATA_FOLDER);
    assert_eq!(
        create_request(&out).config.storage.base_data_folder,
        DEFAULT_DATA_FOLDER
    );
}

#[rstest::rstest]
#[case::empty("", "Type a name for the new profile.")]
#[case::blank("   ", "Type a name for the new profile.")]
#[case::device_name("nul", "nul is a device name on Windows; pick another name")]
#[case::listed_in_another_case("BILLING", "A profile named billing already exists; pick it above.")]
#[case::listed_that_cannot_load(
    "Scratch",
    "A profile named scratch already exists but can't load; choose another name."
)]
fn a_name_that_cannot_be_saved_refuses_the_press_on_the_name_line(
    #[case] name: &str,
    #[case] issue: &str,
) {
    let mut s = typed(open_row(), name);

    let out = s.apply(Input::CreateProfilePressed);

    assert!(out.effects.is_empty());
    assert_eq!(name_issue(&s), Some(issue));
    assert_eq!(row_issue(&s), None);
    assert_eq!(s.save_status, SaveStatus::Idle);
    assert_eq!(s.take_profile_focus(), Some(ProfileFocus::NameField));
}

#[test]
fn a_name_issue_keeps_the_confirm_of_an_earlier_press() {
    let mut s = recalled_row();
    s.apply(Input::CreateProfilePressed);
    s.new_profile_name.clear();

    let out = s.apply(Input::CreateProfilePressed);

    assert!(!creates(&out));
    assert_eq!(name_issue(&s), Some("Type a name for the new profile."));
    assert_eq!(row_issue(&s), Some(RowIssue::RecalledNames));
    assert!(matches!(s.new_profile_line(), Some(RowLine::Confirm(_))));
}

#[test]
fn a_name_against_the_rules_shows_the_rule_without_echoing_the_name() {
    let mut s = typed(open_row(), "SharedAccessKey=x");

    let out = s.apply(Input::CreateProfilePressed);

    let issue = name_issue(&s).unwrap_or_default();
    assert!(out.effects.is_empty());
    assert!(issue.starts_with("A profile name uses letters"), "{issue}");
    assert!(!issue.contains("SharedAccessKey"), "{issue}");
}

#[test]
fn a_form_with_issues_refuses_the_press_and_shows_the_field_issues() {
    let mut s = typed(open_row(), NEW);
    s.form.connection_string = SecretSource::typed();

    let out = s.apply(Input::CreateProfilePressed);

    assert!(out.effects.is_empty());
    assert!(s.show_issues);
    assert_eq!(name_issue(&s), None);
    assert_eq!(row_issue(&s), Some(RowIssue::FormIssues));
    assert_eq!(
        s.new_profile_line(),
        Some(RowLine::Issue(
            "Not saved: fix the fields marked below.".to_owned()
        ))
    );
}

fn client_secret_form(s: &mut AppState) {
    s.form.set_auth(AuthChoice::Credential);
    s.form.credential = CredentialChoice::ClientSecret;
    s.form
        .namespace
        .replace_text("orders.servicebus.windows.net");
    s.form.event_hub.replace_text("orders");
    s.form.tenant_id.replace_text("tenant");
    s.form.client_id.replace_text("client");
    s.form.client_secret = SecretSource::stored();
}

#[rstest::rstest]
#[case::connection_string(false, SecretKind::ConnectionString)]
#[case::client_secret(true, SecretKind::ClientSecret)]
fn a_stored_secret_is_copied_from_the_current_data_folder(
    #[case] client: bool,
    #[case] kind: SecretKind,
) {
    let mut s = typed(open_row(), NEW);
    if client {
        client_secret_form(&mut s);
    } else {
        s.form.connection_string = SecretSource::stored();
    }
    let source = s.data_paths();

    let out = s.apply(Input::CreateProfilePressed);

    let request = create_request(&out);
    assert_eq!(request.stored_from, Some(source));
    assert!(request.given.connection_string.is_none());
    assert_eq!(row_issue(&s), None);
    assert_eq!(s.new_profile_secret(), Some(kind));
}

#[test]
fn typed_secrets_copy_nothing() {
    let mut s = typed(open_row(), NEW);

    let out = s.apply(Input::CreateProfilePressed);

    assert_eq!(create_request(&out).stored_from, None);
    assert_eq!(s.new_profile_secret(), None);
}

#[test]
fn under_a_profile_the_stored_secret_comes_from_the_profiles_data_folder() {
    let mut s = listed();
    let request = load_request(&s.apply(Input::ProfilePicked(profile_home(ORDERS_DEV)))).0;
    s.apply(Input::ProfileLoaded {
        request,
        result: loaded(orders_config()),
    });
    s.apply(Input::NewProfileOpened);
    let mut s = typed(s, NEW);

    let out = s.apply(Input::CreateProfilePressed);

    assert_eq!(s.new_profile_secret(), Some(SecretKind::ConnectionString));
    assert_eq!(
        create_request(&out).stored_from,
        Some(DataPaths::new(profile(ORDERS_DEV).dir().join(".eh-data")))
    );
}

const BOTH_RECALLED: &str = "The namespace and event hub came from the launch folder's \
                             config.json. A profile is trusted by eh send without asking.";

#[test]
fn names_recalled_from_the_launch_folder_ask_to_confirm_and_keep_their_text() {
    let mut s = recalled_row();

    let out = s.apply(Input::CreateProfilePressed);

    assert!(out.effects.is_empty());
    assert_eq!(s.save_status, SaveStatus::Idle);
    assert_eq!(row_issue(&s), Some(RowIssue::RecalledNames));
    assert_eq!(s.form.namespace.text, "orders.servicebus.windows.net");
    assert_eq!(s.form.event_hub.text, "orders");
    assert_eq!(
        s.new_profile_line(),
        Some(RowLine::Confirm(BOTH_RECALLED.to_owned()))
    );
}

#[test]
fn the_recall_marks_show_from_the_moment_the_row_opens() {
    let s = recalled_row();

    assert_eq!(
        s.recall_marks(),
        RecalledFields {
            namespace: true,
            event_hub: true
        }
    );
    assert_eq!(s.new_profile_line(), None);
}

#[test]
fn the_press_after_the_confirm_saves_the_recalled_names() {
    let mut s = recalled_row();
    s.apply(Input::CreateProfilePressed);

    let out = s.apply(Input::CreateProfilePressed);

    let request = create_request(&out);
    assert_eq!(
        request.config.target.namespace.as_deref(),
        Some("orders.servicebus.windows.net")
    );
    assert_eq!(request.config.target.event_hub.as_deref(), Some("orders"));
    assert_eq!(row_issue(&s), None);
}

#[test]
fn return_while_the_confirm_shows_saves_nothing_and_asks_for_save_anyway() {
    let mut s = recalled_row();
    s.apply(Input::CreateProfilePressed);

    let out = s.apply(Input::NewProfileNameSubmitted);

    assert!(out.effects.is_empty() && !out.changed);
    assert_eq!(
        s.take_profile_focus(),
        Some(ProfileFocus::SaveProfileButton)
    );
    assert_eq!(row_issue(&s), Some(RowIssue::RecalledNames));
}

#[test]
fn return_with_recalled_names_and_no_confirm_yet_asks_to_confirm() {
    let mut s = recalled_row();

    let out = s.apply(Input::NewProfileNameSubmitted);

    assert!(out.effects.is_empty());
    assert_eq!(row_issue(&s), Some(RowIssue::RecalledNames));
    assert_eq!(s.take_profile_focus(), None);
}

#[test]
fn return_with_nothing_recalled_saves() {
    let mut s = typed(open_row(), NEW);

    let out = s.apply(Input::NewProfileNameSubmitted);

    create_request(&out);
}

#[test]
fn return_while_the_data_folder_is_held_leaves_focus_in_the_field() {
    let rt = runtime();
    let mut s = recalled_row();
    s.apply(Input::CreateProfilePressed);
    hold(&mut s, Holder::Abandoned, &rt);

    let out = s.apply(Input::NewProfileNameSubmitted);

    assert!(out.effects.is_empty() && !out.changed);
    assert_eq!(s.take_profile_focus(), None);
}

#[test]
fn one_edited_recalled_name_leaves_only_the_other_in_the_confirm_and_the_press_saves() {
    let mut s = recalled_row();
    s.apply(Input::CreateProfilePressed);

    let typed_length = s.form.namespace.text.len();
    s.form.namespace.set_selection(0, typed_length);
    s.form.namespace.insert("billing.servicebus.windows.net");
    let line = s.new_profile_line();
    let marks = s.recall_marks();
    let out = s.apply(Input::CreateProfilePressed);

    assert_eq!(
        line,
        Some(RowLine::Confirm(
            "The event hub came from the launch folder's config.json. A profile is trusted by \
             eh send without asking."
                .to_owned()
        ))
    );
    assert_eq!(
        marks,
        RecalledFields {
            namespace: false,
            event_hub: true
        }
    );
    assert_eq!(
        create_request(&out).config.target.namespace.as_deref(),
        Some("billing.servicebus.windows.net")
    );
}

#[test]
fn once_both_recalled_names_are_edited_the_confirm_goes_and_the_press_saves() {
    let mut s = recalled_row();
    s.apply(Input::CreateProfilePressed);
    s.form.namespace.mark_edited();
    s.form.event_hub.mark_edited();
    let line = s.new_profile_line();

    let out = s.apply(Input::CreateProfilePressed);

    assert_eq!(line, None);
    let request = create_request(&out);
    assert_eq!(request.config.target.event_hub.as_deref(), Some("orders"));
    assert!(s.recall_marks().is_empty());
}

#[test]
fn with_every_recalled_name_edited_a_first_press_saves_without_a_confirm() {
    let mut s = recalled_row();
    s.form.namespace.mark_edited();
    s.form.event_hub.mark_edited();

    let out = s.apply(Input::CreateProfilePressed);

    create_request(&out);
}

#[test]
fn a_form_issue_on_the_second_press_replaces_the_confirm() {
    let mut s = recalled_row();
    s.apply(Input::CreateProfilePressed);
    s.form.consumer_group.clear();

    let out = s.apply(Input::CreateProfilePressed);

    assert!(out.effects.is_empty());
    assert_eq!(row_issue(&s), Some(RowIssue::FormIssues));
}

#[test]
fn a_press_while_a_load_is_pending_keeps_the_confirm() {
    let mut s = recalled_row();
    s.apply(Input::CreateProfilePressed);
    s.apply(Input::ProfilePicked(profile_home(BILLING)));

    let out = s.apply(Input::CreateProfilePressed);

    assert!(out.effects.is_empty() && !out.changed);
    assert_eq!(row_issue(&s), Some(RowIssue::RecalledNames));
}

#[test]
fn a_failed_load_keeps_the_confirm_and_a_landed_load_clears_it() {
    let mut s = recalled_row();
    s.apply(Input::CreateProfilePressed);
    let failed = load_request(&s.apply(Input::ProfilePicked(profile_home(BILLING)))).0;
    s.apply(Input::ProfileLoaded {
        request: failed,
        result: Err("unreadable".to_owned()),
    });
    let kept = row_issue(&s);
    let landed = load_request(&s.apply(Input::ProfilePicked(profile_home(BILLING)))).0;

    s.apply(Input::ProfileLoaded {
        request: landed,
        result: loaded(billing_config()),
    });

    assert_eq!(kept, Some(RowIssue::RecalledNames));
    assert_eq!(row_issue(&s), None);
}

#[rstest::rstest]
#[case::cancel(Input::NewProfileCancelled)]
#[case::close_sheet(Input::CloseSheet)]
#[case::connect(Input::ConnectPressed)]
fn the_confirm_goes_with_the_row(#[case] input: Input) {
    let mut s = recalled_row();
    s.apply(Input::CreateProfilePressed);

    s.apply(input);

    assert!(!is_open(&s));
    assert_eq!(s.new_profile_line(), None);
}

#[rstest::rstest]
#[case::saved(Ok(()))]
#[case::failed(Err("disk full".to_owned()))]
fn a_save_config_while_the_confirm_shows_keeps_it_for_the_next_press(
    #[case] answer: Result<(), String>,
) {
    let mut s = recalled_row();
    s.apply(Input::CreateProfilePressed);
    let save = s.apply(Input::SavePressed);
    let during = s.apply(Input::CreateProfilePressed);

    s.apply(Input::ConfigSaved(answer));
    let line = s.new_profile_line();
    let after = s.apply(Input::CreateProfilePressed);

    save_request(&save);
    assert!(during.effects.is_empty() && !during.changed);
    assert_eq!(line, Some(RowLine::Confirm(BOTH_RECALLED.to_owned())));
    create_request(&after);
}

/// [`open_row`] with [`NEW`] typed and a form the launch folder's `config.json` gave, naming a
/// namespace the typed [`ORDERS`] string contradicts and no event hub.
fn recalled_namespace_row() -> AppState {
    let mut s = open_row();
    let mut config = named_config();
    config.target.namespace = Some("billing.servicebus.windows.net".to_owned());
    config.target.event_hub = None;
    config.target.auth = AuthMethodConfig::ConnectionString {
        stored: StoredSecret::LocalDb,
    };
    s.form = recalled(&ConfigHome::CurrentFolder(launch_folder()), loaded(config)).form;
    s.form.connection_string.edit_mut().replace_text(ORDERS);
    typed(s, NEW)
}

#[test]
fn a_fill_failure_on_save_anyway_keeps_the_confirm() {
    let mut s = recalled_namespace_row();
    s.apply(Input::CreateProfilePressed);

    let out = s.apply(Input::CreateProfilePressed);

    assert!(!creates(&out));
    assert!(matches!(s.save_status, SaveStatus::Failed(_)));
    assert_eq!(row_issue(&s), Some(RowIssue::RecalledNames));
    assert!(matches!(s.new_profile_line(), Some(RowLine::Confirm(_))));
}

#[test]
fn after_a_fill_failure_return_still_only_moves_focus_to_save_anyway() {
    let mut s = recalled_namespace_row();
    s.apply(Input::NewProfileNameSubmitted);
    s.apply(Input::CreateProfilePressed);

    let out = s.apply(Input::NewProfileNameSubmitted);

    assert!(out.effects.is_empty());
    assert_eq!(
        s.take_profile_focus(),
        Some(ProfileFocus::SaveProfileButton)
    );
    assert_eq!(row_issue(&s), Some(RowIssue::RecalledNames));
}

#[test]
fn after_a_failed_create_the_next_press_with_recalled_names_asks_again() {
    let mut s = recalled_row();
    s.apply(Input::CreateProfilePressed);
    let request = create_request(&s.apply(Input::CreateProfilePressed)).request;
    s.apply(Input::ProfileCreated {
        request,
        result: Err("disk full".to_owned()),
    });

    let out = s.apply(Input::CreateProfilePressed);

    assert!(!creates(&out));
    assert_eq!(row_issue(&s), Some(RowIssue::RecalledNames));
}

#[test]
fn a_typed_connection_string_fills_the_blank_names_of_the_new_profile() {
    let mut s = typed(open_row(), NEW);

    let out = s.apply(Input::CreateProfilePressed);

    let request = create_request(&out);
    assert_eq!(
        request.config.target.namespace.as_deref(),
        Some("orders.servicebus.windows.net")
    );
    assert_eq!(request.config.target.event_hub.as_deref(), Some("orders"));
    assert!(
        s.form.namespace.text.is_empty(),
        "the form keeps its fields"
    );
}

/// [`open_row`] with [`NEW`] typed and a namespace the typed string's endpoint contradicts.
fn contradicted_row() -> AppState {
    let mut s = typed(open_row(), NEW);
    s.form.namespace.insert("billing.servicebus.windows.net");
    s
}

#[test]
fn a_fill_that_fails_ends_the_press_as_a_failed_save() {
    let mut s = contradicted_row();

    let out = s.apply(Input::CreateProfilePressed);

    assert!(!creates(&out));
    assert!(matches!(s.save_status, SaveStatus::Failed(_)));
    let toasts = s.toasts().entries();
    assert_eq!(toasts.len(), 1, "{toasts:?}");
    assert!(
        toasts[0]
            .options
            .text
            .starts_with("Could not save profile orders-new: The configured namespace"),
        "{}",
        toasts[0].options.text
    );
    assert!(!toasts[0].options.text.contains("bGlzdGVu"));
    assert!(is_open(&s));
}

#[test]
fn the_destination_line_follows_the_form() {
    let mut s = typed(open_row(), NEW);
    let from_string = s.new_profile_destination();
    s.form.namespace.insert("billing.servicebus.windows.net");
    let invalid = s.new_profile_destination();
    s.form.event_hub.insert("invoices");
    let typed_names = s.new_profile_destination();
    s.form.consumer_group.clear();
    let with_issues = s.new_profile_destination();

    assert_eq!(
        from_string,
        ProfileDestination::FromString(Names {
            namespace: "orders.servicebus.windows.net".to_owned(),
            event_hub: "orders".to_owned(),
        })
    );
    assert_eq!(invalid, ProfileDestination::Invalid);
    assert_eq!(
        typed_names,
        ProfileDestination::Typed(Names {
            namespace: "billing.servicebus.windows.net".to_owned(),
            event_hub: "invoices".to_owned(),
        })
    );
    assert_eq!(with_issues, ProfileDestination::None);
}

#[test]
fn a_stored_string_with_a_blank_name_names_no_destination() {
    let mut s = typed(open_row(), NEW);
    s.form.connection_string = SecretSource::stored();

    assert_eq!(s.new_profile_destination(), ProfileDestination::None);
}

#[test]
fn a_whitespace_only_name_field_takes_the_filled_name() {
    let mut s = typed(open_row(), NEW);
    s.form.event_hub.insert("   ");

    let out = s.apply(Input::CreateProfilePressed);

    assert_eq!(
        create_request(&out).config.target.event_hub.as_deref(),
        Some("orders")
    );
}

fn save_request(out: &Outcome) -> &SaveRequest {
    only(out, |effect| match effect {
        Effect::SaveConfig(request) => Some(request),
        _ => None,
    })
}

/// [`listed`] switched to `orders-dev` (a stored string, no names) with [`ORDERS`] typed.
fn orders_dev_home() -> AppState {
    let mut s = listed();
    let request = load_request(&s.apply(Input::ProfilePicked(profile_home(ORDERS_DEV)))).0;
    s.apply(Input::ProfileLoaded {
        request,
        result: loaded(orders_config()),
    });
    s.form.connection_string.edit_mut().replace_text(ORDERS);
    s
}

#[test]
fn save_config_into_a_profile_fills_the_names_as_typed_and_a_second_save_keeps_them() {
    let mut s = orders_dev_home();
    s.form.event_hub.insert("  ");

    let first = s.apply(Input::SavePressed);
    let saved = save_request(&first).config.target.clone();
    s.apply(Input::ConfigSaved(Ok(())));
    let second = s.apply(Input::SavePressed);

    assert_eq!(
        (saved.namespace.as_deref(), saved.event_hub.as_deref()),
        (Some("orders.servicebus.windows.net"), Some("orders"))
    );
    assert_eq!(s.form.namespace.text, "orders.servicebus.windows.net");
    assert_eq!(s.form.event_hub.text, "orders");
    assert!(!s.form.namespace.is_pristine() && !s.form.event_hub.is_pristine());
    assert!(s.form.connection_string.is_stored(), "the save settled");
    assert_eq!(save_request(&second).config.target, saved);
}

#[test]
fn save_config_with_the_launch_folder_home_fills_nothing() {
    let mut s = listed();

    let out = s.apply(Input::SavePressed);

    let target = &save_request(&out).config.target;
    assert_eq!(
        (target.namespace.as_deref(), target.event_hub.as_deref()),
        (None, None)
    );
    assert!(s.form.namespace.text.is_empty());
}

#[test]
fn a_fill_that_fails_ends_save_config_as_a_failed_save() {
    let mut s = orders_dev_home();
    s.form.namespace.insert("billing.servicebus.windows.net");

    let out = s.apply(Input::SavePressed);

    assert!(out.effects.is_empty());
    assert!(matches!(s.save_status, SaveStatus::Failed(_)));
    let toasts = s.toasts().entries();
    assert_eq!(toasts.len(), 1, "{toasts:?}");
    assert!(
        toasts[0]
            .options
            .text
            .starts_with("Could not save config.json: The configured namespace"),
        "{}",
        toasts[0].options.text
    );
    assert!(s.form.event_hub.text.is_empty());
}

#[test]
fn a_fixed_cause_clears_the_row_line_before_the_next_press() {
    let mut s = typed(open_row(), NEW);
    s.form.connection_string = SecretSource::typed();
    s.apply(Input::CreateProfilePressed);

    s.form.connection_string.edit_mut().replace_text(ORDERS);

    assert_eq!(row_issue(&s), Some(RowIssue::FormIssues));
    assert_eq!(s.new_profile_line(), None);
}

#[test]
fn a_profile_load_and_a_launch_folder_without_a_config_leave_no_recalled_names() {
    let mut s = listed();
    let request = load_request(&s.apply(Input::ProfilePicked(profile_home(BILLING)))).0;
    s.apply(Input::ProfileLoaded {
        request,
        result: loaded(billing_config()),
    });

    let missing = recalled(
        &ConfigHome::CurrentFolder(launch_folder()),
        Ok(ConfigLoad::Missing),
    );

    assert_eq!(s.form.namespace.text, "billing.servicebus.windows.net");
    assert!(s.form.recalled_fields().is_empty());
    assert!(missing.form.recalled_fields().is_empty());
}

#[test]
fn a_press_with_the_row_closed_changes_nothing() {
    let mut s = typed(listed(), NEW);

    let out = s.apply(Input::CreateProfilePressed);

    assert!(out.effects.is_empty() && !out.changed);
    assert_eq!(s.save_status, SaveStatus::Idle);
}

#[test]
fn a_press_while_a_save_runs_changes_nothing() {
    let mut s = typed(open_row(), NEW);
    s.apply(Input::SavePressed);

    let out = s.apply(Input::CreateProfilePressed);

    assert!(out.effects.is_empty() && !out.changed);
    assert!(matches!(s.save_status, SaveStatus::Saving(SaveTo::Home(_))));
}

#[test]
fn a_press_while_a_profile_loads_changes_nothing() {
    let mut s = typed(open_row(), NEW);
    s.apply(Input::ProfilePicked(profile_home(BILLING)));

    let out = s.apply(Input::CreateProfilePressed);

    assert!(out.effects.is_empty() && !out.changed);
    assert_eq!(s.save_status, SaveStatus::Idle);
}

/// What holds the data folder in a lock case.
#[derive(Clone, Copy, Debug)]
enum Holder {
    Session,
    Abandoned,
    BulkExport,
    MessageExport,
    Sending,
}

/// Makes `holder` hold the data folder of `s` without closing its sheet.
fn hold(s: &mut AppState, holder: Holder, rt: &tokio::runtime::Runtime) {
    match holder {
        Holder::Session => {
            let (session, _) = detached_session(1, rt.handle());
            s.connection = ConnectionState::Connected {
                session,
                partitions: Vec::new(),
            };
        }
        Holder::Abandoned => s.abandoned_attempts.push(9),
        Holder::BulkExport => {
            s.archive.bulk = BulkExport::Running {
                export: 1,
                cancel: Arc::new(AtomicBool::new(false)),
                cancel_requested: false,
                counts: Default::default(),
            };
        }
        Holder::MessageExport => {
            s.read.inspector.export = ExportPhase::Running {
                owner: 1,
                key: MessageKey {
                    partition_id: "0".to_owned(),
                    sequence_number: 1,
                },
                key_str: "p0-s1".to_owned(),
            };
        }
        Holder::Sending => {
            s.send.run = SendRun::Preparing {
                run: 1,
                cancel: Arc::new(AtomicBool::new(false)),
                cancel_requested: false,
                requested: 1,
                pacing: Pacing::batched(),
                target: None,
            };
        }
    }
    assert!(s.data_folder_lock().is_some(), "{holder:?} holds nothing");
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[rstest::rstest]
#[case::session(Holder::Session)]
#[case::abandoned_attempt(Holder::Abandoned)]
#[case::bulk_export(Holder::BulkExport)]
#[case::message_export(Holder::MessageExport)]
#[case::sending(Holder::Sending)]
fn a_press_while_the_data_folder_is_held_changes_nothing(#[case] holder: Holder) {
    let rt = runtime();
    let mut s = typed(open_row(), NEW);
    hold(&mut s, holder, &rt);

    let out = s.apply(Input::CreateProfilePressed);

    assert!(out.effects.is_empty() && !out.changed);
    assert_eq!(s.save_status, SaveStatus::Idle);
    assert_eq!(name_issue(&s), None);
}

#[test]
fn a_press_without_a_workspace_changes_nothing() {
    let mut s = typed(open_row(), NEW);
    s.set_launch(launch_folder(), Err("No home folder".to_owned()));
    s.new_profile = NewProfileRow::Open {
        name_issue: None,
        row_issue: None,
    };

    let out = s.apply(Input::CreateProfilePressed);

    assert!(out.effects.is_empty() && !out.changed);
    assert_eq!(s.save_status, SaveStatus::Idle);
}

#[test]
fn the_row_does_not_open_without_a_workspace() {
    let mut s = state();
    s.set_launch(launch_folder(), Err("No home folder".to_owned()));
    s.apply(Input::OpenSheet(SheetPurpose::Connect));

    let out = s.apply(Input::NewProfileOpened);

    assert!(!out.changed);
    assert!(!is_open(&s));
    assert_eq!(s.take_profile_focus(), None);
}

#[rstest::rstest]
#[case::held(true)]
#[case::loading(false)]
fn the_row_does_not_open_under_the_lock_or_while_a_profile_loads(#[case] held: bool) {
    let rt = runtime();
    let mut s = listed();
    if held {
        hold(&mut s, Holder::Abandoned, &rt);
    } else {
        s.apply(Input::ProfilePicked(profile_home(BILLING)));
    }

    let out = s.apply(Input::NewProfileOpened);

    assert!(!out.changed);
    assert!(!is_open(&s));
}

#[test]
fn a_name_edit_clears_the_name_issue() {
    let mut s = open_row();
    s.apply(Input::CreateProfilePressed);

    s.new_profile_name.insert("o");
    s.apply(Input::NewProfileNameEdited);

    assert_eq!(name_issue(&s), None);
    assert!(is_open(&s));
}

#[test]
fn a_load_clears_both_issues() {
    let mut s = open_row();
    s.new_profile = NewProfileRow::Open {
        name_issue: Some("Type a name for the new profile.".to_owned()),
        row_issue: Some(RowIssue::RecalledNames),
    };

    let request = load_request(&s.apply(Input::ProfilePicked(profile_home(BILLING)))).0;
    s.apply(Input::ProfileLoaded {
        request,
        result: loaded(billing_config()),
    });

    assert_eq!(
        s.new_profile_row(),
        &NewProfileRow::Open {
            name_issue: None,
            row_issue: None
        }
    );
}

#[test]
fn a_saved_profile_wipes_the_typed_secrets_closes_the_row_lists_again_and_opens_it() {
    let (mut s, request) = creating();
    let new = profile(NEW);

    let out = s.apply(Input::ProfileCreated {
        request,
        result: Ok(new.clone()),
    });

    assert!(s.form.connection_string.edit().text.is_empty());
    assert_eq!(s.form.typed_secret(), None);
    assert_eq!(s.send.destination, Destination::Missing);
    assert_eq!(s.save_status, SaveStatus::Idle);
    assert!(!is_open(&s));
    assert!(s.new_profile_name.text.is_empty());
    list_request(&out);
    assert_eq!(load_request(&out).1, new.dir());
    assert_eq!(
        s.profile_note(),
        Some(&ProfileNote::Created(NEW.to_owned()))
    );
    assert_eq!(s.take_profile_focus(), Some(ProfileFocus::SaveAsButton));
}

#[test]
fn the_saved_profiles_load_switches_the_home_and_data_folder_with_no_cleared_secret_note() {
    let (mut s, request) = creating();
    let new = profile(NEW);
    let out = s.apply(Input::ProfileCreated {
        request,
        result: Ok(new.clone()),
    });
    let (load, _) = load_request(&out);
    let mut config = orders_config();
    config.target.namespace = None;

    let out = s.apply(Input::ProfileLoaded {
        request: load,
        result: loaded(config),
    });

    assert_eq!(s.home(), &ConfigHome::Profile(new.clone()));
    let switched = only(&out, |effect| match effect {
        Effect::SwitchDataFolder(paths) => Some(paths.clone()),
        _ => None,
    });
    assert_eq!(switched, DataPaths::new(new.dir().join(".eh-data")));
    assert_eq!(
        s.profile_note(),
        Some(&ProfileNote::Created(NEW.to_owned()))
    );
}

#[test]
fn a_saved_profile_whose_folder_is_the_current_home_still_loads() {
    let mut s = listed();
    let request = load_request(&s.apply(Input::ProfilePicked(profile_home(ORDERS_DEV)))).0;
    s.apply(Input::ProfileLoaded {
        request,
        result: loaded(orders_config()),
    });
    s.apply(Input::CloseSheet);
    let request = list_request(&s.apply(Input::OpenSheet(SheetPurpose::Connect)));
    s.apply(Input::ProfilesListed {
        request,
        result: Ok(vec![ready(BILLING, billing_config())]),
    });
    s.apply(Input::NewProfileOpened);
    s.form.connection_string.edit_mut().replace_text(ORDERS);
    let mut s = typed(s, ORDERS_DEV);
    let request = create_request(&s.apply(Input::CreateProfilePressed)).request;

    let out = s.apply(Input::ProfileCreated {
        request,
        result: Ok(profile(ORDERS_DEV)),
    });

    assert_eq!(load_request(&out).1, profile_home(ORDERS_DEV).dir());
}

#[test]
fn a_saved_profile_while_a_bulk_export_runs_is_not_opened_and_the_list_refreshes() {
    let rt = runtime();
    let (mut s, request) = creating();
    hold(&mut s, Holder::BulkExport, &rt);

    let out = s.apply(Input::ProfileCreated {
        request,
        result: Ok(profile(NEW)),
    });

    assert!(
        !out.effects
            .iter()
            .any(|effect| matches!(effect, Effect::LoadProfile { .. }))
    );
    list_request(&out);
    assert_eq!(s.send.destination, Destination::Missing);
    assert_eq!(s.home(), &ConfigHome::CurrentFolder(launch_folder()));
    assert_eq!(
        s.profile_note(),
        Some(&ProfileNote::CreatedNotSwitched {
            name: NEW.to_owned(),
            reason: BULK_LOCK
        })
    );
    assert_eq!(s.take_profile_focus(), None);
}

#[test]
fn the_not_switched_note_says_where_the_typed_secret_went() {
    let note = ProfileNote::CreatedNotSwitched {
        name: NEW.to_owned(),
        reason: BULK_LOCK,
    };

    assert_eq!(
        note.text(),
        "Saved profile orders-new, with what you typed. Not switched. The data folder can change \
         once the bulk export ends. Pick it under Profile in the connection sheet to use it."
    );
}

#[test]
fn a_failed_save_toasts_marks_the_footer_and_keeps_the_row_and_home() {
    let (mut s, request) = creating();

    s.apply(Input::ProfileCreated {
        request,
        result: Err("disk full".to_owned()),
    });

    let toasts = s.toasts().entries();
    assert_eq!(toasts.len(), 1, "{toasts:?}");
    assert_eq!(toasts[0].options.kind, ToastKind::Danger);
    assert_eq!(
        toasts[0].options.text,
        "Could not save profile orders-new: disk full"
    );
    assert_eq!(s.save_status, SaveStatus::Failed("disk full".to_owned()));
    assert!(is_open(&s));
    assert_eq!(s.new_profile_name.text, NEW);
    assert_eq!(s.home(), &ConfigHome::CurrentFolder(launch_folder()));
    assert_eq!(s.form.connection_string.edit().text, ORDERS);
}

#[test]
fn an_answer_for_another_create_changes_nothing() {
    let (mut s, request) = creating();

    let out = s.apply(Input::ProfileCreated {
        request: request + 1,
        result: Ok(profile(NEW)),
    });

    assert!(out.effects.is_empty() && !out.changed);
    assert_eq!(s.save_status.creating().map(ProfileName::as_str), Some(NEW));
    assert_eq!(s.form.connection_string.edit().text, ORDERS);
}

#[test]
fn an_answer_with_no_create_running_changes_nothing() {
    let mut s = open_row();

    let out = s.apply(Input::ProfileCreated {
        request: 1,
        result: Err("disk full".to_owned()),
    });

    assert!(out.effects.is_empty() && !out.changed);
    assert_eq!(s.save_status, SaveStatus::Idle);
    assert!(s.toasts().entries().is_empty());
}

#[rstest::rstest]
#[case::saved(Ok(()))]
#[case::failed(Err("disk full".to_owned()))]
fn a_config_save_answer_while_a_profile_saves_keeps_the_create(#[case] result: Result<(), String>) {
    let (mut s, _) = creating();

    let out = s.apply(Input::ConfigSaved(result));

    assert!(!out.changed);
    assert_eq!(s.save_status.creating().map(ProfileName::as_str), Some(NEW));
    assert!(s.toasts().entries().is_empty());
}

#[test]
fn a_failed_config_save_with_nothing_saving_still_toasts() {
    let mut s = open_row();

    s.apply(Input::ConfigSaved(Err("disk full".to_owned())));

    assert_eq!(s.toasts().entries().len(), 1);
    assert_eq!(s.save_status, SaveStatus::Failed("disk full".to_owned()));
}

#[test]
fn cancel_closes_the_row_and_asks_for_focus_on_save_as_profile() {
    let mut s = typed(open_row(), NEW);

    s.apply(Input::NewProfileCancelled);

    assert!(!is_open(&s));
    assert_eq!(s.take_profile_focus(), Some(ProfileFocus::SaveAsButton));
}

#[test]
fn cancel_while_the_profile_saves_changes_nothing_and_moves_no_focus() {
    let (mut s, _) = creating();

    let out = s.apply(Input::NewProfileCancelled);

    assert!(!out.changed);
    assert!(is_open(&s));
    assert_eq!(s.take_profile_focus(), None);
}

#[test]
fn cancel_with_the_row_closed_changes_nothing() {
    let mut s = listed();

    let out = s.apply(Input::NewProfileCancelled);

    assert!(!out.changed);
    assert_eq!(s.take_profile_focus(), None);
}

#[test]
fn a_sheet_close_closes_the_row() {
    let mut s = typed(open_row(), NEW);

    s.apply(Input::CloseSheet);

    assert!(!is_open(&s));
}

fn quits(out: &Outcome) -> bool {
    out.effects
        .iter()
        .any(|effect| matches!(effect, Effect::Quit))
}

#[test]
fn a_window_closed_while_the_profile_saves_quits_once_the_answer_lands() {
    let (mut s, request) = creating();

    let closed = s.apply(Input::WindowClosed(Utc::now()));
    let answered = s.apply(Input::ProfileCreated {
        request,
        result: Ok(profile(NEW)),
    });

    assert!(!quits(&closed));
    assert!(quits(&answered));
}

#[rstest::rstest]
#[case::created(false, "Profile saved")]
#[case::not_switched(true, "Profile not switched")]
fn an_answer_with_the_sheet_closed_toasts_its_note(#[case] held: bool, #[case] title: &str) {
    let rt = runtime();
    let (mut s, request) = creating();
    s.apply(Input::CloseSheet);
    if held {
        hold(&mut s, Holder::BulkExport, &rt);
    }

    s.apply(Input::ProfileCreated {
        request,
        result: Ok(profile(NEW)),
    });

    let toasts = s.toasts().entries();
    assert_eq!(toasts.len(), 1, "{toasts:?}");
    assert_eq!(toasts[0].options.kind, ToastKind::Info);
    assert_eq!(toasts[0].options.title, Some(title));
    assert!(s.profile_note().is_none());
    assert_eq!(s.take_profile_focus(), None);
}

#[test]
fn opening_the_row_again_keeps_the_typed_name() {
    let mut s = typed(open_row(), NEW);

    let out = s.apply(Input::NewProfileOpened);

    assert!(!out.changed);
    assert_eq!(s.new_profile_name.text, NEW);
    assert_eq!(s.take_profile_focus(), None);
}

#[test]
fn the_row_does_not_open_with_the_sheet_closed() {
    let mut s = with_profiles(state());

    let out = s.apply(Input::NewProfileOpened);

    assert!(!out.changed);
    assert!(!is_open(&s));
    assert_eq!(s.take_profile_focus(), None);
}

#[test]
fn a_pick_and_a_connect_during_the_save_leave_the_profile_saved_but_not_opened() {
    let (mut s, request) = creating();
    let picked = s.apply(Input::ProfilePicked(profile_home(BILLING)));
    s.apply(Input::ConnectPressed);

    let out = s.apply(Input::ProfileCreated {
        request,
        result: Ok(profile(NEW)),
    });

    assert!(picked.effects.is_empty() && !picked.changed);
    assert!(
        !out.effects
            .iter()
            .any(|effect| matches!(effect, Effect::LoadProfile { .. }))
    );
    assert_eq!(s.form.typed_secret(), None);
    let toasts = s.toasts().entries();
    assert_eq!(toasts.len(), 1, "{toasts:?}");
    assert_eq!(toasts[0].options.title, Some("Profile not switched"));
    assert!(
        toasts[0]
            .options
            .text
            .contains("The data folder can change once the session ends."),
        "{}",
        toasts[0].options.text
    );
}

#[test]
fn a_sheet_reopened_during_the_save_gets_no_focus_request_when_it_lands() {
    let (mut s, request) = creating();
    s.apply(Input::CloseSheet);
    s.apply(Input::OpenSheet(SheetPurpose::Connect));

    s.apply(Input::ProfileCreated {
        request,
        result: Ok(profile(NEW)),
    });

    assert_eq!(s.take_profile_focus(), None);
}

#[test]
fn a_press_goes_out_only_once_at_a_time() {
    let (mut s, _) = creating();
    s.new_profile_name.insert("2");

    let again = s.apply(Input::CreateProfilePressed);

    assert!(!creates(&again));
}
