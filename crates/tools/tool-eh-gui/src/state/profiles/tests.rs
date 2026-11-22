use super::*;
use crate::archive::bulk::BulkExport;
use crate::archive::results::Results;
use crate::bridge::test_support::detached_session;
use crate::form::{AuthChoice, SecretSource};
use crate::read::LogKind;
use crate::read::inspector::ExportPhase;
use crate::send::Destination;
use crate::send::run::SendRun;
use crate::state::{Failure, Input, SheetPurpose};
use crate::test_harness::builders::{
    BILLING, ORDERS, ORDERS_DEV, ORDERS_KEYWORD, billing_config, connected, launch_folder, listing,
    orders_config, profile_home, state, test_workspace, with_profiles,
};
use chrono::Utc;
use shared_eventhub_core::filters::TextFilter;
use shared_eventhub_core::models::MessageKey;
use shared_eventhub_core::producer::Pacing;
use shared_eventhub_persistence::config::ToolConfig;
use shared_eventhub_persistence::export::{DiskWriteMode, ExportFormat, ExportOutcome};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

const ABANDONED_LOCK: &str = "The data folder can change once the cancelled connection stops.";

fn launch_home() -> ConfigHome {
    ConfigHome::CurrentFolder(launch_folder())
}

fn loaded(config: ToolConfig) -> Result<ConfigLoad, String> {
    Ok(ConfigLoad::Loaded(Box::new(config)))
}

/// The one listing `out` asks for.
fn list_request(out: &Outcome) -> u64 {
    match out.effects.as_slice() {
        [Effect::ListProfiles { request, .. }] => *request,
        other => panic!("expected ListProfiles, got {other:?}"),
    }
}

/// The one load `out` asks for: its request and folder.
fn load_request(out: &Outcome) -> (u64, PathBuf) {
    match out.effects.as_slice() {
        [Effect::LoadProfile { request, dir }] => (*request, dir.clone()),
        other => panic!("expected LoadProfile, got {other:?}"),
    }
}

/// A first launch over the test workspace with a typed connection string, the sheet open, and
/// the workspace listed.
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

/// Picks `home`; returns the load request.
fn pick(s: &mut AppState, home: ConfigHome) -> u64 {
    let out = s.apply(Input::ProfilePicked(home));
    load_request(&out).0
}

/// Picks `home` and answers its load with `result`; returns the answer's outcome.
fn switch(s: &mut AppState, home: ConfigHome, result: Result<ConfigLoad, String>) -> Outcome {
    let request = pick(s, home);
    s.apply(Input::ProfileLoaded { request, result })
}

fn switched_to(out: &Outcome) -> Option<&DataPaths> {
    out.effects.iter().find_map(|effect| match effect {
        Effect::SwitchDataFolder(paths) => Some(paths),
        _ => None,
    })
}

#[test]
fn a_pick_while_disconnected_loads_the_profiles_folder() {
    let mut s = listed();

    let out = s.apply(Input::ProfilePicked(profile_home(ORDERS_DEV)));

    let (_, dir) = load_request(&out);
    assert_eq!(dir, profile_home(ORDERS_DEV).dir());
    assert!(s.profile_switch().is_loading());
    assert_eq!(s.home(), &launch_home());
}

/// What holds the data folder in a lock case.
#[derive(Clone, Copy, Debug)]
enum Holder {
    Connecting,
    Connected,
    Stopping,
    Saving,
    BulkExport,
    MessageExport,
    Sending,
    Abandoned,
}

/// [`listed`] with `holder` holding the data folder; `rt` runs a session's commands.
fn held(holder: Holder, rt: &tokio::runtime::Runtime) -> AppState {
    let mut s = match holder {
        Holder::Connected | Holder::Stopping => with_profiles(connected(rt)),
        Holder::Connecting
        | Holder::Saving
        | Holder::BulkExport
        | Holder::MessageExport
        | Holder::Sending
        | Holder::Abandoned => listed(),
    };
    match holder {
        Holder::Connecting => drop(s.apply(Input::ConnectPressed)),
        Holder::Connected => {}
        Holder::Stopping => drop(s.apply(Input::DisconnectPressed)),
        Holder::Saving => drop(s.apply(Input::SavePressed)),
        Holder::BulkExport => {
            s.archive.bulk = BulkExport::Running {
                export: 1,
                cancel: Arc::new(AtomicBool::new(false)),
                cancel_requested: false,
                counts: ExportOutcome::default(),
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
        Holder::Abandoned => {
            s.apply(Input::ConnectPressed);
            s.apply(Input::DisconnectPressed);
        }
    }
    assert!(s.data_folder_lock().is_some(), "{holder:?} holds nothing");
    s
}

#[rstest::rstest]
#[case::connecting(Holder::Connecting)]
#[case::connected(Holder::Connected)]
#[case::stopping(Holder::Stopping)]
#[case::saving(Holder::Saving)]
#[case::bulk_export(Holder::BulkExport)]
#[case::message_export(Holder::MessageExport)]
#[case::sending(Holder::Sending)]
#[case::abandoned_attempt(Holder::Abandoned)]
fn a_pick_while_the_data_folder_is_held_changes_nothing(#[case] holder: Holder) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut s = held(holder, &rt);

    let out = s.apply(Input::ProfilePicked(profile_home(ORDERS_DEV)));

    assert!(out.effects.is_empty() && !out.changed);
    assert!(!s.profile_switch().is_loading());
    assert_eq!(s.home(), &launch_home());
}

#[test]
fn a_loaded_profile_replaces_the_form_rules_and_export_options_and_moves_the_data_folder() {
    let mut s = listed();
    s.connection = ConnectionState::Failed(Failure::starting("earlier"));
    s.show_issues = true;
    s.save_status = SaveStatus::Saved;
    s.read
        .log(Utc::now(), LogKind::Error, None, "boom".to_owned(), false);
    s.archive.results = Results::Loading { generation: 1 };

    let out = switch(&mut s, profile_home(ORDERS_DEV), loaded(orders_config()));

    let profile = profile_home(ORDERS_DEV);
    assert_eq!(s.home(), &profile);
    assert_eq!(
        switched_to(&out),
        Some(&DataPaths::new(profile.dir().join(".eh-data")))
    );
    assert!(s.form.connection_string.is_stored());
    assert_eq!(s.form.consumer_group.text, "audit");
    assert_eq!(
        s.read.rules_applied.text,
        Some(TextFilter::Contains(vec![ORDERS_KEYWORD.to_owned()]))
    );
    assert_eq!(s.read.inspector.format, ExportFormat::Json);
    assert!(!s.read.inspector.metadata);
    assert_eq!(s.archive.options.format, ExportFormat::Json);
    assert!(matches!(s.archive.results, Results::NotSearched));
    assert_eq!(s.send.destination, Destination::Stored);
    assert_eq!(s.save_status, SaveStatus::Idle);
    assert!(matches!(s.connection, ConnectionState::Disconnected));
    assert!(!s.show_issues);
    assert_eq!(s.read.session_log.len(), 1);
    assert!(
        s.config_caption
            .as_deref()
            .is_some_and(|caption| caption.starts_with("Loaded from profile orders-dev: ")),
        "{:?}",
        s.config_caption
    );
    assert!(!s.profile_switch().is_loading());
}

/// The one save `out` asks for: its folder and data folder.
fn saved_to(out: &Outcome) -> (PathBuf, DataPaths) {
    match out.effects.as_slice() {
        [Effect::SaveConfig(request)] => (request.dir.clone(), request.paths.clone()),
        other => panic!("expected SaveConfig, got {other:?}"),
    }
}

#[test]
fn a_save_after_a_switch_writes_to_the_profile_then_back_to_the_launch_folder() {
    let mut s = listed();
    switch(&mut s, profile_home(BILLING), loaded(billing_config()));

    let in_profile = saved_to(&s.apply(Input::SavePressed));
    s.apply(Input::ConfigSaved(Ok(())));
    switch(&mut s, launch_home(), Ok(ConfigLoad::Missing));
    s.form.connection_string.edit_mut().replace_text(ORDERS);
    let in_launch = saved_to(&s.apply(Input::SavePressed));

    let billing = profile_home(BILLING);
    assert_eq!(
        in_profile,
        (
            billing.dir().to_path_buf(),
            DataPaths::new(billing.dir().join(".eh-data"))
        )
    );
    assert_eq!(in_launch, (launch_folder(), DataPaths::new(".eh-data")));
}

#[test]
fn an_older_loads_answer_changes_nothing_and_the_latest_pick_wins() {
    let mut s = listed();
    let first = pick(&mut s, profile_home(BILLING));
    let second = pick(&mut s, profile_home(ORDERS_DEV));

    let stale = s.apply(Input::ProfileLoaded {
        request: first,
        result: loaded(billing_config()),
    });
    let stale_home = s.home().clone();
    let current = s.apply(Input::ProfileLoaded {
        request: second,
        result: loaded(orders_config()),
    });

    assert!(stale.effects.is_empty() && !stale.changed);
    assert_eq!(stale_home, launch_home());
    assert!(switched_to(&current).is_some());
    assert_eq!(s.home(), &profile_home(ORDERS_DEV));
}

#[test]
fn an_answer_with_no_load_pending_changes_nothing() {
    let mut s = listed();

    let out = s.apply(Input::ProfileLoaded {
        request: 1,
        result: loaded(orders_config()),
    });

    assert!(out.effects.is_empty() && !out.changed);
    assert_eq!(s.home(), &launch_home());
}

#[test]
fn picking_the_current_home_drops_the_pending_load() {
    let mut s = listed();
    let pending = pick(&mut s, profile_home(BILLING));

    let back = s.apply(Input::ProfilePicked(launch_home()));
    let late = s.apply(Input::ProfileLoaded {
        request: pending,
        result: loaded(billing_config()),
    });

    assert!(back.effects.is_empty());
    assert!(!s.profile_switch().is_loading());
    assert!(late.effects.is_empty() && !late.changed);
    assert_eq!(s.home(), &launch_home());
}

#[test]
fn picking_the_current_home_with_nothing_pending_changes_nothing() {
    let mut s = listed();

    let out = s.apply(Input::ProfilePicked(launch_home()));

    assert!(out.effects.is_empty() && !out.changed);
}

#[rstest::rstest]
#[case::connect(Input::ConnectPressed)]
#[case::save(Input::SavePressed)]
#[case::data_folder(Input::DataFolderPicked(PathBuf::from("elsewhere")))]
fn while_a_profile_loads_connect_save_and_a_folder_pick_do_nothing(#[case] input: Input) {
    let mut s = listed();
    pick(&mut s, profile_home(BILLING));

    let out = s.apply(input);

    assert!(out.effects.is_empty() && !out.changed);
    assert!(matches!(s.connection, ConnectionState::Disconnected));
    assert_eq!(s.save_status, SaveStatus::Idle);
    assert_eq!(s.form.data_folder, ".eh-data");
}

#[test]
fn an_answer_while_the_data_folder_is_held_leaves_a_note_and_nothing_else() {
    let mut s = listed();
    let request = pick(&mut s, profile_home(ORDERS_DEV));
    s.abandoned_attempts.push(9);

    let out = s.apply(Input::ProfileLoaded {
        request,
        result: loaded(orders_config()),
    });

    assert!(switched_to(&out).is_none());
    assert_eq!(s.home(), &launch_home());
    assert_eq!(s.form.connection_string.edit().text, ORDERS);
    assert_eq!(
        s.profile_note(),
        Some(&ProfileNote::NotSwitched(ABANDONED_LOCK))
    );
    assert!(!s.profile_switch().is_loading());
}

#[rstest::rstest]
#[case::unreadable(Err("'billing/config.json' is an eh config, but it is not valid".to_owned()))]
#[case::gone(Ok(ConfigLoad::Missing))]
#[case::foreign(Ok(ConfigLoad::Foreign))]
fn a_profile_that_cannot_be_opened_leaves_the_home_and_says_why(
    #[case] result: Result<ConfigLoad, String>,
) {
    let mut s = listed();

    let out = switch(&mut s, profile_home(BILLING), result);

    assert!(switched_to(&out).is_none());
    assert_eq!(s.home(), &launch_home());
    assert_eq!(s.form.connection_string.edit().text, ORDERS);
    let note = s.profile_note().map(ProfileNote::text).unwrap_or_default();
    assert!(
        note.starts_with("Couldn't open profile billing: ")
            && note.ends_with("Pick another profile or fix its config.json."),
        "{note}"
    );
}

#[test]
fn a_launch_folder_that_cannot_be_read_still_switches_back_with_defaults() {
    let mut s = listed();
    switch(&mut s, profile_home(BILLING), loaded(billing_config()));

    let out = switch(
        &mut s,
        launch_home(),
        Err("Failed to read 'config.json'".to_owned()),
    );

    assert_eq!(s.home(), &launch_home());
    assert_eq!(switched_to(&out), Some(&DataPaths::new(".eh-data")));
    assert_eq!(s.form.auth, AuthChoice::ConnectionString);
    assert_eq!(
        s.config_caption.as_deref(),
        Some("config.json was not loaded: Failed to read 'config.json'")
    );
}

#[test]
fn a_launch_folder_with_no_config_switches_back_with_defaults_and_no_caption() {
    let mut s = listed();
    switch(&mut s, profile_home(BILLING), loaded(billing_config()));

    switch(&mut s, launch_home(), Ok(ConfigLoad::Missing));

    assert_eq!(s.home(), &launch_home());
    assert_eq!(s.form.auth, AuthChoice::ConnectionString);
    assert!(s.config_caption.is_none());
}

#[test]
fn a_switch_drops_a_typed_secret_and_says_so() {
    let mut s = listed();

    switch(&mut s, profile_home(BILLING), loaded(billing_config()));

    assert_eq!(
        s.profile_note(),
        Some(&ProfileNote::SecretCleared(SecretKind::ConnectionString))
    );
    assert!(s.form.typed_secret().is_none());
    assert!(s.form.to_config().1.connection_string.is_none());
}

#[test]
fn a_switch_that_drops_a_typed_client_secret_names_it() {
    let mut s = listed();
    s.form.connection_string = SecretSource::stored();
    s.form
        .client_secret
        .edit_mut()
        .replace_text("client-marker");

    switch(&mut s, profile_home(BILLING), loaded(billing_config()));

    assert_eq!(
        s.profile_note().map(ProfileNote::text).as_deref(),
        Some("The client secret you typed was cleared; it wasn't saved.")
    );
}

#[test]
fn a_switch_from_a_stored_secret_leaves_no_note() {
    let mut s = listed();
    s.form.connection_string = SecretSource::stored();

    switch(&mut s, profile_home(BILLING), loaded(billing_config()));

    assert_eq!(s.profile_note(), None);
}

#[test]
fn a_sheet_close_drops_the_note() {
    let mut s = listed();
    switch(&mut s, profile_home(BILLING), loaded(billing_config()));

    s.apply(Input::CloseSheet);

    assert_eq!(s.profile_note(), None);
}

/// [`listed`] with its sheet closed before the answer to a pick of `orders-dev` lands with
/// `result`.
fn answered_with_the_sheet_closed(result: Result<ConfigLoad, String>, held: bool) -> AppState {
    let mut s = listed();
    let request = pick(&mut s, profile_home(ORDERS_DEV));
    s.apply(Input::CloseSheet);
    if held {
        s.abandoned_attempts.push(9);
    }
    s.apply(Input::ProfileLoaded { request, result });
    s
}

#[rstest::rstest]
#[case::load_failed(Err("disk gone".to_owned()), false, ToastKind::Danger, None)]
#[case::not_switched(
    loaded(orders_config()),
    true,
    ToastKind::Info,
    Some("Profile not switched")
)]
#[case::secret_cleared(
    loaded(orders_config()),
    false,
    ToastKind::Info,
    Some("Profile switched")
)]
fn a_note_that_lands_with_the_sheet_closed_toasts_instead(
    #[case] result: Result<ConfigLoad, String>,
    #[case] held: bool,
    #[case] kind: ToastKind,
    #[case] title: Option<&str>,
) {
    let s = answered_with_the_sheet_closed(result, held);

    let toasts = s.toasts().entries();

    assert_eq!(toasts.len(), 1, "{toasts:?}");
    assert_eq!(toasts[0].options.kind, kind);
    assert_eq!(toasts[0].options.title, title);
    assert!(s.profile_note().is_none());
}

#[test]
fn a_note_that_lands_with_the_sheet_open_raises_no_toast() {
    let mut s = listed();

    switch(
        &mut s,
        profile_home(ORDERS_DEV),
        Err("disk gone".to_owned()),
    );

    assert!(s.toasts().entries().is_empty());
    assert!(s.profile_note().is_some_and(ProfileNote::is_failure));
}

#[test]
fn a_toasted_note_carries_the_notes_text() {
    let s = answered_with_the_sheet_closed(Err("disk gone".to_owned()), false);

    let text = &s.toasts().entries()[0].options.text;

    assert_eq!(
        text,
        "Couldn't open profile orders-dev: disk gone. Pick another profile or fix its \
         config.json."
    );
}

#[rstest::rstest]
#[case::started(true)]
#[case::failed(false)]
fn a_connect_cancelled_while_preparing_holds_the_data_folder_until_it_answers(
    #[case] started: bool,
) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut s = listed();
    s.apply(Input::ConnectPressed);
    s.apply(Input::DisconnectPressed);
    let held = s.data_folder_lock();
    let result = if started {
        Ok(detached_session(1, rt.handle()).0)
    } else {
        Err("refused".to_owned())
    };

    let out = s.apply(Input::SessionReady { attempt: 1, result });

    assert_eq!(held, Some(ABANDONED_LOCK));
    assert!(out.changed);
    assert_eq!(s.data_folder_lock(), None);
    assert!(s.abandoned_attempts().is_empty());
    assert_eq!(
        out.effects
            .iter()
            .any(|effect| matches!(effect, Effect::StopOrphan(_))),
        started
    );
}

#[test]
fn an_answer_for_another_attempt_keeps_the_hold() {
    let mut s = listed();
    s.apply(Input::ConnectPressed);
    s.apply(Input::DisconnectPressed);

    let out = s.apply(Input::SessionReady {
        attempt: 7,
        result: Err("refused".to_owned()),
    });

    assert!(!out.changed);
    assert_eq!(s.data_folder_lock(), Some(ABANDONED_LOCK));
}

#[test]
fn two_cancelled_connects_hold_the_data_folder_until_both_answer() {
    let mut s = listed();
    for _ in 0..2 {
        s.apply(Input::ConnectPressed);
        s.apply(Input::DisconnectPressed);
    }
    let refused = |attempt| Input::SessionReady {
        attempt,
        result: Err("refused".to_owned()),
    };

    s.apply(refused(2));
    let after_second = s.data_folder_lock();
    s.apply(refused(1));

    assert_eq!(after_second, Some(ABANDONED_LOCK));
    assert_eq!(s.data_folder_lock(), None);
}

#[test]
fn a_window_close_while_preparing_records_the_attempt() {
    let mut s = listed();
    s.apply(Input::ConnectPressed);

    s.apply(Input::WindowClosed(Utc::now()));

    assert_eq!(s.abandoned_attempts(), [1]);
    assert_eq!(s.data_folder_lock(), Some(ABANDONED_LOCK));
}

#[test]
fn a_stale_listing_is_dropped() {
    let mut s = with_profiles(state());
    let first = list_request(&s.apply(Input::OpenSheet(SheetPurpose::Connect)));
    s.apply(Input::CloseSheet);
    s.apply(Input::OpenSheet(SheetPurpose::Connect));

    let out = s.apply(Input::ProfilesListed {
        request: first,
        result: Ok(listing()),
    });

    assert!(!out.changed);
    assert!(matches!(
        s.profile_list(),
        ProfileList::Loading { previous: None, .. }
    ));
}

#[test]
fn while_listing_again_the_previous_entries_stay_listed() {
    let mut s = listed();
    s.apply(Input::CloseSheet);

    let out = s.apply(Input::OpenSheet(SheetPurpose::Connect));

    list_request(&out);
    assert_eq!(s.profile_list().entries().map(<[_]>::len), Some(3));
    assert!(s.home_for_label(ORDERS_DEV).is_some());
}

#[test]
fn a_failed_listing_keeps_its_reason_and_lists_nothing() {
    let mut s = with_profiles(state());
    let request = list_request(&s.apply(Input::OpenSheet(SheetPurpose::Connect)));

    s.apply(Input::ProfilesListed {
        request,
        result: Err("Failed to list the workspace".to_owned()),
    });

    assert!(matches!(s.profile_list(), ProfileList::Failed(reason) if reason.contains("list")));
    assert!(s.profile_list().entries().is_none());
}

#[test]
fn with_no_workspace_the_sheet_lists_nothing_and_offers_only_the_launch_folder() {
    let mut s = state();
    s.set_launch(launch_folder(), Err("No home folder".to_owned()));

    let out = s.apply(Input::OpenSheet(SheetPurpose::Connect));

    assert!(out.effects.is_empty());
    assert!(matches!(s.profile_list(), ProfileList::NotLoaded));
    assert_eq!(s.workspace(), Err("No home folder"));
    assert_eq!(s.home_for_label(LAUNCH_FOLDER), Some(launch_home()));
    assert_eq!(s.home_for_label(ORDERS_DEV), None);
}

#[test]
fn a_label_for_a_profile_that_cannot_load_names_no_home() {
    let s = listed();

    assert_eq!(s.home_for_label("scratch"), None);
}

#[test]
fn the_launch_workspace_is_the_one_set() {
    let s = with_profiles(state());

    assert_eq!(s.workspace(), Ok(&test_workspace()));
    assert_eq!(s.launch_folder(), launch_folder());
}

const OUTSIDE: &str = "/elsewhere/data";

fn outside_data_folder(storage: &mut StorageConfig) {
    storage.base_data_folder = OUTSIDE.to_owned();
}

fn outside_export_folder(storage: &mut StorageConfig) {
    storage.export.export_folder = OUTSIDE.to_owned();
}

fn outside_received_path(storage: &mut StorageConfig) {
    storage.disk_write.received_msg_path = OUTSIDE.to_owned();
    storage.disk_write.mode = DiskWriteMode::Immediate;
}

fn every_folder_outside(storage: &mut StorageConfig) {
    outside_data_folder(storage);
    outside_export_folder(storage);
    outside_received_path(storage);
}

/// [`orders_config`] with its storage changed by `change`.
fn orders_config_with(change: fn(&mut StorageConfig)) -> ToolConfig {
    let mut config = orders_config();
    change(&mut config.storage);
    config
}

#[rstest::rstest]
#[case::data_folder(
    outside_data_folder,
    "config.json names a data folder outside this folder; it was not used."
)]
#[case::export_folder(
    outside_export_folder,
    "config.json names an export folder outside this folder; it was not used."
)]
#[case::received_message_path(
    outside_received_path,
    "config.json names a received message path outside this folder; it was not used. Writing \
     message files is off."
)]
#[case::all_three(
    every_folder_outside,
    "config.json names a data folder, an export folder, and a received message path outside \
     this folder; they were not used. Writing message files is off."
)]
fn a_launch_folder_config_gives_the_default_for_each_folder_it_names_outside(
    #[case] change: fn(&mut StorageConfig),
    #[case] caption: &str,
) {
    let given = recalled(&launch_home(), loaded(orders_config_with(change)));

    let inside = orders_config().storage;
    assert_eq!(given.form.data_folder, inside.base_data_folder);
    assert_eq!(
        given.form.received_msg_path.text,
        inside.disk_write.received_msg_path
    );
    assert_eq!(given.form.disk_write, DiskWriteMode::Off);
    assert_eq!(given.form.to_config().0.storage, inside);
    assert_eq!(given.export, inside.export);
    assert_eq!(given.folders_caption.as_deref(), Some(caption));
}

#[test]
fn two_folders_outside_the_launch_folder_are_both_named() {
    let given = recalled(
        &launch_home(),
        loaded(orders_config_with(|storage| {
            outside_data_folder(storage);
            outside_export_folder(storage);
        })),
    );

    assert_eq!(
        given.folders_caption.as_deref(),
        Some(
            "config.json names a data folder and an export folder outside this folder; they \
             were not used."
        )
    );
}

#[test]
fn a_received_message_path_inside_the_launch_folder_keeps_its_disk_write_mode() {
    let given = recalled(
        &launch_home(),
        loaded(orders_config_with(|storage| {
            storage.disk_write.received_msg_path = "dumps".to_owned();
            storage.disk_write.mode = DiskWriteMode::AtEnd;
        })),
    );

    assert_eq!(given.form.disk_write, DiskWriteMode::AtEnd);
    assert_eq!(given.form.received_msg_path.text, "dumps");
    assert_eq!(given.folders_caption, None);
}

#[test]
fn a_profile_keeps_the_folders_it_names_outside_its_folder() {
    let config = orders_config_with(every_folder_outside);

    let given = recalled(&profile_home(ORDERS_DEV), loaded(config.clone()));

    assert_eq!(given.form.to_config().0.storage, config.storage);
    assert_eq!(given.export, config.storage.export);
    assert_eq!(given.folders_caption, None);
}

#[test]
fn a_return_to_a_launch_folder_whose_config_names_folders_outside_it_uses_the_defaults() {
    let mut s = listed();
    switch(&mut s, profile_home(BILLING), loaded(billing_config()));

    let out = switch(
        &mut s,
        launch_home(),
        loaded(orders_config_with(every_folder_outside)),
    );

    assert_eq!(s.home(), &launch_home());
    assert_eq!(switched_to(&out), Some(&DataPaths::new(".eh-data")));
    assert_eq!(s.form.to_config().0.storage, orders_config().storage);
    assert_eq!(s.archive.options.export_folder, "exports");
    assert!(
        s.folders_caption
            .as_deref()
            .is_some_and(|caption| caption.starts_with("config.json names a data folder, ")),
        "{:?}",
        s.folders_caption
    );
}

/// [`listed`] back on a launch folder whose `config.json` names every folder outside it.
fn with_the_folders_caption() -> AppState {
    let mut s = listed();
    switch(&mut s, profile_home(BILLING), loaded(billing_config()));
    switch(
        &mut s,
        launch_home(),
        loaded(orders_config_with(every_folder_outside)),
    );
    assert!(s.folders_caption.is_some());
    s
}

#[test]
fn the_next_load_replaces_the_folders_caption() {
    let mut s = with_the_folders_caption();

    switch(&mut s, profile_home(BILLING), loaded(billing_config()));

    assert_eq!(s.folders_caption, None);
}

#[test]
fn picking_a_data_folder_clears_the_folders_caption() {
    let mut s = with_the_folders_caption();

    s.apply(Input::DataFolderPicked(PathBuf::from("picked")));

    assert_eq!(s.folders_caption, None);
    assert_eq!(s.form.data_folder, "picked");
}

#[test]
fn a_save_config_that_succeeds_clears_the_folders_caption() {
    let mut s = with_the_folders_caption();
    let saved_into = saved_to(&s.apply(Input::SavePressed)).0;

    s.apply(Input::ConfigSaved(Ok(())));

    assert_eq!(saved_into, launch_folder());
    assert_eq!(s.save_status, SaveStatus::Saved);
    assert_eq!(s.folders_caption, None);
}

#[test]
fn a_save_config_that_fails_keeps_the_folders_caption() {
    let mut s = with_the_folders_caption();
    let caption = s.folders_caption.clone();
    s.apply(Input::SavePressed);

    s.apply(Input::ConfigSaved(Err("disk full".to_owned())));

    assert!(matches!(s.save_status, SaveStatus::Failed(_)));
    assert_eq!(s.folders_caption, caption);
}

fn not_saved(setting: &str) -> SaveStatus {
    SaveStatus::Failed(format!(
        "The {setting} is outside this folder; config.json here keeps only folders inside. Pick \
         one inside or use a profile."
    ))
}

/// An absolute launch folder that nothing creates.
fn absolute_launch_folder() -> PathBuf {
    std::env::temp_dir().join("eh-gui-test-launch")
}

/// An absolute folder outside [`absolute_launch_folder`] that nothing creates.
fn outside_folder() -> PathBuf {
    std::env::temp_dir().join("eh-gui-test-outside")
}

/// [`listed`] launched in [`absolute_launch_folder`].
fn launched() -> AppState {
    let mut s = listed();
    s.set_launch(absolute_launch_folder(), Ok(test_workspace()));
    s
}

/// The config the one save `out` asks for writes.
fn saved_config(out: &Outcome) -> &ToolConfig {
    match out.effects.as_slice() {
        [Effect::SaveConfig(request)] => &request.config,
        other => panic!("expected SaveConfig, got {other:?}"),
    }
}

#[test]
fn save_config_into_the_launch_folder_refuses_a_data_folder_picked_outside_it() {
    let mut s = launched();
    s.apply(Input::DataFolderPicked(outside_folder()));

    let out = s.apply(Input::SavePressed);

    assert!(out.effects.is_empty(), "{:?}", out.effects);
    assert_eq!(s.save_status, not_saved("data folder"));
    let toasts = s.toasts().entries();
    assert_eq!(toasts.len(), 1, "{toasts:?}");
    assert_eq!(toasts[0].options.kind, ToastKind::Danger);
    assert!(
        toasts[0]
            .options
            .text
            .starts_with("Could not save config.json: The data folder is outside this folder"),
        "{}",
        toasts[0].options.text
    );
}

#[test]
fn a_data_folder_picked_inside_the_launch_folder_is_held_and_saved_as_the_relative_remainder() {
    let mut s = launched();
    let remainder = PathBuf::from("captures").join("today");

    let picked = s.apply(Input::DataFolderPicked(
        absolute_launch_folder().join(&remainder),
    ));
    let saved = s.apply(Input::SavePressed);

    let remainder = remainder.display().to_string();
    assert_eq!(s.form.data_folder, remainder);
    assert_eq!(switched_to(&picked), Some(&DataPaths::new(&remainder)));
    assert_eq!(saved_config(&saved).storage.base_data_folder, remainder);
}

#[rstest::rstest]
#[case::immediate(DiskWriteMode::Immediate)]
#[case::at_end(DiskWriteMode::AtEnd)]
fn save_config_into_the_launch_folder_refuses_an_absolute_received_message_path_with_message_files_on(
    #[case] mode: DiskWriteMode,
) {
    let mut s = launched();
    s.form.disk_write = mode;
    s.form
        .received_msg_path
        .replace_text(outside_folder().display().to_string());

    let out = s.apply(Input::SavePressed);

    assert!(out.effects.is_empty(), "{:?}", out.effects);
    assert_eq!(s.save_status, not_saved("received message path"));
}

#[rstest::rstest]
#[case::absolute(outside_folder().display().to_string())]
#[case::emptied(String::new())]
fn save_config_into_the_launch_folder_with_message_files_off_saves_the_default_received_message_path(
    #[case] left_behind: String,
) {
    let mut s = launched();
    s.form.disk_write = DiskWriteMode::Off;
    s.form.received_msg_path.replace_text(left_behind);

    let out = s.apply(Input::SavePressed);

    let saved = &saved_config(&out).storage.disk_write;
    assert_eq!(saved.mode, DiskWriteMode::Off);
    assert_eq!(
        saved.received_msg_path,
        StorageConfig::default().disk_write.received_msg_path
    );
    assert!(s.save_status.is_saving());
}

#[test]
fn save_config_into_a_profile_with_message_files_off_keeps_an_absolute_received_message_path() {
    let mut s = launched();
    switch(&mut s, profile_home(BILLING), loaded(billing_config()));
    let outside = outside_folder().display().to_string();
    s.form.disk_write = DiskWriteMode::Off;
    s.form.received_msg_path.replace_text(outside.clone());

    let out = s.apply(Input::SavePressed);

    assert_eq!(
        saved_config(&out).storage.disk_write.received_msg_path,
        outside
    );
}

#[test]
fn save_config_into_the_launch_folder_refuses_an_export_folder_outside_it() {
    let mut s = launched();
    s.form = ConnectForm::from_config(&orders_config_with(outside_export_folder)).0;

    let out = s.apply(Input::SavePressed);

    assert!(out.effects.is_empty(), "{:?}", out.effects);
    assert_eq!(s.save_status, not_saved("export folder"));
}

#[test]
fn a_pick_inside_the_launch_folder_clears_a_refused_save_and_the_next_save_runs() {
    let mut s = launched();
    s.apply(Input::DataFolderPicked(outside_folder()));
    s.apply(Input::SavePressed);

    s.apply(Input::DataFolderPicked(
        absolute_launch_folder().join("captures"),
    ));
    let cleared = s.save_status.clone();
    let out = s.apply(Input::SavePressed);

    assert_eq!(cleared, SaveStatus::Idle);
    assert_eq!(saved_config(&out).storage.base_data_folder, "captures");
    assert!(s.save_status.is_saving());
}

#[test]
fn a_data_folder_pick_keeps_a_saved_status() {
    let mut s = launched();
    s.apply(Input::SavePressed);
    s.apply(Input::ConfigSaved(Ok(())));

    s.apply(Input::DataFolderPicked(
        absolute_launch_folder().join("captures"),
    ));

    assert_eq!(s.save_status, SaveStatus::Saved);
}

#[test]
fn under_a_profile_a_data_folder_picked_outside_is_kept_as_given_and_saved() {
    let mut s = launched();
    switch(&mut s, profile_home(BILLING), loaded(billing_config()));
    let outside = outside_folder().display().to_string();

    s.apply(Input::DataFolderPicked(outside_folder()));
    let out = s.apply(Input::SavePressed);

    assert_eq!(s.form.data_folder, outside);
    assert_eq!(saved_config(&out).storage.base_data_folder, outside);
}

#[test]
fn under_a_profile_a_data_folder_picked_inside_the_launch_folder_is_kept_as_given() {
    let mut s = launched();
    switch(&mut s, profile_home(BILLING), loaded(billing_config()));
    let picked = absolute_launch_folder().join("captures");

    s.apply(Input::DataFolderPicked(picked.clone()));

    assert_eq!(s.form.data_folder, picked.display().to_string());
}

#[test]
fn the_launch_folder_itself_picked_as_the_data_folder_is_kept_as_given_and_not_saved() {
    let mut s = launched();

    s.apply(Input::DataFolderPicked(absolute_launch_folder()));
    let out = s.apply(Input::SavePressed);

    assert_eq!(
        s.form.data_folder,
        absolute_launch_folder().display().to_string()
    );
    assert!(out.effects.is_empty(), "{:?}", out.effects);
    assert_eq!(s.save_status, not_saved("data folder"));
}

#[test]
fn connect_after_a_data_folder_picked_outside_the_launch_folder_prepares_a_session() {
    let mut s = launched();
    s.apply(Input::DataFolderPicked(outside_folder()));

    let out = s.apply(Input::ConnectPressed);

    let [Effect::PrepareSession { request, .. }] = out.effects.as_slice() else {
        panic!("expected PrepareSession, got {:?}", out.effects);
    };
    assert_eq!(request.paths, DataPaths::new(outside_folder()));
}

#[test]
fn a_profile_s_relative_data_folder_sits_in_the_profile_folder() {
    let mut s = listed();
    switch(&mut s, profile_home(ORDERS_DEV), loaded(orders_config()));

    s.form.data_folder = "captures".to_owned();

    assert_eq!(
        s.data_paths(),
        DataPaths::new(profile_home(ORDERS_DEV).dir().join("captures"))
    );
}
