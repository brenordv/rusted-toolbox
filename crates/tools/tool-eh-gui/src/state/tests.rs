use super::*;
use crate::archive::bulk::BulkExport;
use crate::archive::test_support::{messages as archive_messages, page as archive_page};
use crate::bridge::test_support::detached_session;
use crate::form::SecretSource;
use crate::read::inspector::ExportPhase;
use shared_eventhub_core::engine::{EngineCommand, PartitionEnd};
use shared_eventhub_core::models::MessageKey;
use shared_eventhub_core::stats::{StatsSnapshot, StatsTracker};
use shared_eventhub_persistence::export::ExportOutcome;
use std::collections::BTreeMap;
use tokio::sync::mpsc;

const SECRET: &str = "Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKey=marker";

struct Fixture {
    rt: tokio::runtime::Runtime,
    state: AppState,
}

fn typed_secret() -> SecretSource {
    let mut secret = SecretSource::typed();
    secret.edit_mut().replace_text(SECRET);
    secret
}

fn typed_text(state: &AppState) -> &str {
    &state.form.connection_string.edit().text
}

fn fixture() -> Fixture {
    let mut form = ConnectForm::default();
    form.connection_string = typed_secret();
    Fixture {
        rt: tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
        state: AppState::new(None, form, None, FileLogging::Off),
    }
}

fn base_time() -> DateTime<Utc> {
    Utc::now()
}

fn summary(end: RunEnd, failed: &[&str]) -> RunSummary {
    let mut partitions = BTreeMap::new();
    for partition_id in failed {
        partitions.insert(
            (*partition_id).to_owned(),
            PartitionEnd::Failed("boom".to_owned()),
        );
    }
    RunSummary {
        stats: StatsTracker::new(Utc::now()).snapshot(),
        partitions,
        end,
    }
}

fn clean_finish() -> Result<ExportOutcome, String> {
    Ok(ExportOutcome::default())
}

impl Fixture {
    fn apply(&mut self, input: Input) -> Outcome {
        self.state.apply(input)
    }

    /// Connect pressed; returns the attempt.
    fn preparing(&mut self) -> u64 {
        let out = self.apply(Input::ConnectPressed);
        match out.effects.as_slice() {
            [Effect::PrepareSession { attempt, .. }] => *attempt,
            other => panic!("expected PrepareSession, got {other:?}"),
        }
    }

    /// Connect pressed and the session started; returns the session's command receiver.
    fn with_session(&mut self) -> (u64, mpsc::Receiver<EngineCommand>) {
        let attempt = self.preparing();
        let (session, commands) = detached_session(attempt, self.rt.handle());
        self.apply(Input::SessionReady {
            attempt,
            result: Ok(session),
        });
        (attempt, commands)
    }

    fn connected(&mut self) -> (u64, mpsc::Receiver<EngineCommand>) {
        let (id, commands) = self.with_session();
        self.apply(delivery(
            id,
            EngineEvent::Started {
                partitions: vec!["0".to_owned()],
            },
        ));
        (id, commands)
    }

    fn stopping(&mut self) -> u64 {
        let (id, _) = self.connected();
        self.apply(Input::DisconnectPressed);
        id
    }

    fn failure(&self) -> &Failure {
        match &self.state.connection {
            ConnectionState::Failed(failure) => failure,
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    fn failed_text(&self) -> &str {
        &self.failure().text
    }
}

fn delivery(session: u64, event: EngineEvent) -> Input {
    Input::Delivery(UiDelivery {
        session,
        at: Utc::now(),
        events: vec![event],
        stats: None,
    })
}

fn done(
    session: u64,
    engine: Result<RunSummary, String>,
    finish: Result<ExportOutcome, String>,
) -> Input {
    Input::EngineDone(EngineDone {
        session,
        engine,
        finish,
    })
}

fn has_quit(out: &Outcome) -> bool {
    out.effects
        .iter()
        .any(|effect| matches!(effect, Effect::Quit))
}

fn has_reopen(out: &Outcome) -> bool {
    out.effects
        .iter()
        .any(|effect| matches!(effect, Effect::ReopenWindow))
}

fn has_stop(out: &Outcome) -> bool {
    out.effects
        .iter()
        .any(|effect| matches!(effect, Effect::StopSession))
}

#[test]
fn connect_from_disconnected_prepares_a_session_for_the_form() {
    let mut f = fixture();

    let out = f.apply(Input::ConnectPressed);

    let [Effect::PrepareSession { attempt, request }] = out.effects.as_slice() else {
        panic!("expected PrepareSession, got {:?}", out.effects);
    };
    assert_eq!(*attempt, 1);
    assert_eq!(
        request.given.connection_string.as_ref().unwrap().expose(),
        SECRET
    );
    assert_eq!(request.paths, f.state.data_paths());
    assert!(matches!(
        f.state.connection,
        ConnectionState::Connecting {
            attempt: 1,
            session: None,
            ..
        }
    ));
}

#[test]
fn connect_with_form_issues_does_nothing() {
    let mut f = fixture();
    f.state.form = ConnectForm::default();

    let out = f.apply(Input::ConnectPressed);

    assert!(out.effects.is_empty());
    assert!(matches!(f.state.connection, ConnectionState::Disconnected));
}

#[test]
fn connect_while_a_session_is_starting_is_rejected() {
    let mut f = fixture();
    f.preparing();

    let out = f.apply(Input::ConnectPressed);

    assert!(out.effects.is_empty());
    assert!(matches!(
        f.state.connection,
        ConnectionState::Connecting { attempt: 1, .. }
    ));
}

#[test]
fn connect_while_connected_is_rejected() {
    let mut f = fixture();
    f.connected();

    let out = f.apply(Input::ConnectPressed);

    assert!(out.effects.is_empty());
    assert!(matches!(
        f.state.connection,
        ConnectionState::Connected { .. }
    ));
}

#[test]
fn connect_from_failed_starts_a_new_attempt() {
    let mut f = fixture();
    f.state.connection = ConnectionState::Failed(Failure::starting("earlier"));

    let attempt = f.preparing();

    assert_eq!(attempt, 1);
}

#[test]
fn a_started_session_is_held_and_the_typed_secret_cleared() {
    let mut f = fixture();

    f.with_session();

    assert!(matches!(
        f.state.connection,
        ConnectionState::Connecting {
            session: Some(_),
            ..
        }
    ));
    assert!(typed_text(&f.state).is_empty());
}

#[rstest::rstest]
#[case::read_shows_it_in_place(Section::Read, 0)]
#[case::export_needs_a_toast(Section::Export, 1)]
#[case::send_needs_a_toast(Section::Send, 1)]
fn a_session_that_cannot_start_fails_with_its_cause_and_toasts_where_read_is_hidden(
    #[case] section: Section,
    #[case] toasts: usize,
) {
    let mut f = fixture();
    let attempt = f.preparing();
    f.state.section = section;

    f.apply(Input::SessionReady {
        attempt,
        result: Err("The archive is in use by another eh or eh-gui".to_owned()),
    });

    assert_eq!(
        f.failed_text(),
        "The archive is in use by another eh or eh-gui"
    );
    assert_eq!(f.failure().stage, FailureStage::Starting);
    assert_eq!(f.state.toasts().entries().len(), toasts);
    assert!(
        f.state
            .toasts()
            .entries()
            .iter()
            .all(|t| t.options.kind == ToastKind::Danger)
    );
}

#[test]
fn a_session_ready_after_its_attempt_was_cancelled_is_stopped() {
    let mut f = fixture();
    let attempt = f.preparing();
    f.apply(Input::DisconnectPressed);
    let (late, mut commands) = detached_session(attempt, f.rt.handle());

    let out = f.apply(Input::SessionReady {
        attempt,
        result: Ok(late),
    });
    let [Effect::StopOrphan(orphan)] = out.effects.as_slice() else {
        panic!("expected StopOrphan, got {:?}", out.effects);
    };
    orphan.stop();

    assert!(matches!(commands.try_recv(), Ok(EngineCommand::Stop)));
    assert!(matches!(f.state.connection, ConnectionState::Disconnected));
}

#[test]
fn a_stale_failed_session_ready_is_ignored() {
    let mut f = fixture();
    f.state.connection = ConnectionState::Failed(Failure::starting("current"));

    let out = f.apply(Input::SessionReady {
        attempt: 9,
        result: Err("old".to_owned()),
    });

    assert!(out.effects.is_empty());
    assert_eq!(f.failed_text(), "current");
}

#[test]
fn disconnect_while_preparing_abandons_the_attempt() {
    let mut f = fixture();
    f.preparing();

    let out = f.apply(Input::DisconnectPressed);

    assert!(out.effects.is_empty());
    assert!(matches!(f.state.connection, ConnectionState::Disconnected));
}

#[test]
fn closing_the_window_while_preparing_quits() {
    let mut f = fixture();
    f.preparing();

    let out = f.apply(Input::WindowClosed(base_time()));

    assert!(has_quit(&out));
    assert!(matches!(f.state.connection, ConnectionState::Disconnected));
}

#[test]
fn a_started_event_connects_the_session() {
    let mut f = fixture();

    f.connected();

    assert!(matches!(
        &f.state.connection,
        ConnectionState::Connected { partitions, .. } if partitions == &["0".to_owned()]
    ));
}

#[test]
fn disconnect_while_connected_stops_the_session() {
    let mut f = fixture();
    let (_, mut commands) = f.connected();

    let out = f.apply(Input::DisconnectPressed);
    crate::app::run_effects(&mut f.state, out.effects);

    assert!(matches!(
        f.state.connection,
        ConnectionState::Stopping { .. }
    ));
    assert!(matches!(commands.try_recv(), Ok(EngineCommand::Stop)));
}

#[test]
fn disconnect_while_the_session_connects_stops_it() {
    let mut f = fixture();
    f.with_session();

    let out = f.apply(Input::DisconnectPressed);

    assert!(has_stop(&out));
    assert!(matches!(
        f.state.connection,
        ConnectionState::Stopping { .. }
    ));
}

#[test]
fn a_started_event_while_stopping_is_ignored() {
    let mut f = fixture();
    let (id, _) = f.with_session();
    f.apply(Input::DisconnectPressed);

    f.apply(delivery(id, EngineEvent::Started { partitions: vec![] }));

    assert!(matches!(
        f.state.connection,
        ConnectionState::Stopping { .. }
    ));
}

#[test]
fn closing_the_window_while_connected_stops_and_waits() {
    let mut f = fixture();
    f.connected();

    let out = f.apply(Input::WindowClosed(base_time()));

    assert!(has_stop(&out));
    assert!(!has_quit(&out));
    assert!(f.state.window_is_closed());
    assert!(matches!(
        f.state.connection,
        ConnectionState::Stopping { .. }
    ));
}

#[test]
fn closing_the_window_while_stopping_keeps_waiting() {
    let mut f = fixture();
    f.stopping();

    let out = f.apply(Input::WindowClosed(base_time()));

    assert!(has_stop(&out));
    assert!(!has_quit(&out));
}

#[test]
fn a_completed_run_disconnects_with_an_info_toast() {
    let mut f = fixture();
    let (id, _) = f.connected();

    f.apply(done(
        id,
        Ok(summary(RunEnd::Completed, &[])),
        clean_finish(),
    ));

    assert!(matches!(f.state.connection, ConnectionState::Disconnected));
    assert_eq!(f.state.toasts().entries()[0].options.kind, ToastKind::Info);
}

#[test]
fn a_clean_stop_disconnects_without_a_toast() {
    let mut f = fixture();
    let id = f.stopping();

    f.apply(done(id, Ok(summary(RunEnd::Stopped, &[])), clean_finish()));

    assert!(matches!(f.state.connection, ConnectionState::Disconnected));
    assert!(f.state.toasts().entries().is_empty());
}

#[test]
fn a_clean_stop_after_the_window_closed_quits() {
    let mut f = fixture();
    let (id, _) = f.connected();
    f.apply(Input::WindowClosed(base_time()));

    let out = f.apply(done(id, Ok(summary(RunEnd::Stopped, &[])), clean_finish()));

    assert!(has_quit(&out));
}

#[test]
fn a_stop_with_a_failed_partition_fails_naming_it() {
    let mut f = fixture();
    let id = f.stopping();

    f.apply(done(
        id,
        Ok(summary(RunEnd::Stopped, &["1", "3"])),
        clean_finish(),
    ));

    assert_eq!(f.failed_text(), "Partitions 1, 3 failed before the stop");
    assert_eq!(f.failure().stage, FailureStage::Run);
}

#[test]
fn a_failed_run_fails_with_its_reason() {
    let mut f = fixture();
    let (id, _) = f.connected();

    f.apply(done(
        id,
        Ok(summary(
            RunEnd::Failed("partitions 0 failed".to_owned()),
            &["0"],
        )),
        clean_finish(),
    ));

    assert_eq!(f.failed_text(), "The run failed: partitions 0 failed");
}

#[test]
fn unwritten_message_files_fail_the_session() {
    let mut f = fixture();
    let id = f.stopping();

    f.apply(done(
        id,
        Ok(summary(RunEnd::Stopped, &[])),
        Ok(ExportOutcome {
            failed: 2,
            ..ExportOutcome::default()
        }),
    ));

    assert!(
        f.failed_text()
            .starts_with("2 message file(s) could not be written"),
        "{}",
        f.failed_text()
    );
}

#[test]
fn a_fatal_start_error_and_a_failed_finish_keep_both_texts() {
    let mut f = fixture();
    let (id, _) = f.with_session();
    f.apply(delivery(
        id,
        EngineEvent::Error {
            partition_id: None,
            error: "Event Hubs namespace 'x' does not have one of the required suffixes".to_owned(),
            fatal: true,
        },
    ));

    f.apply(done(
        id,
        Err("Failed to connect: the namespace was rejected".to_owned()),
        Err("the archive could not be read".to_owned()),
    ));

    assert_eq!(
        f.failed_text(),
        "Event Hubs namespace 'x' does not have one of the required suffixes; Failed to write \
         the at-end message files: the archive could not be read"
    );
}

#[test]
fn another_sessions_delivery_and_done_are_ignored() {
    let mut f = fixture();
    let (id, _) = f.with_session();

    let started = f.apply(delivery(
        id + 1,
        EngineEvent::Started { partitions: vec![] },
    ));
    let ended = f.apply(done(id + 1, Err("old".to_owned()), clean_finish()));

    assert!(!started.changed);
    assert!(!ended.changed);
    assert!(matches!(
        f.state.connection,
        ConnectionState::Connecting {
            session: Some(_),
            ..
        }
    ));
}

#[test]
fn closing_the_window_with_no_session_quits() {
    let mut f = fixture();

    let out = f.apply(Input::WindowClosed(base_time()));

    assert!(has_quit(&out));
}

#[test]
fn closing_the_window_after_a_failure_quits() {
    let mut f = fixture();
    f.state.connection = ConnectionState::Failed(Failure::starting("earlier"));

    let out = f.apply(Input::WindowClosed(base_time()));

    assert!(has_quit(&out));
}

#[test]
fn a_failure_after_the_window_closed_reopens_it_and_a_second_close_quits() {
    let mut f = fixture();
    let (id, _) = f.connected();
    f.apply(Input::WindowClosed(base_time()));

    let failed = f.apply(done(
        id,
        Ok(summary(RunEnd::Stopped, &["0"])),
        clean_finish(),
    ));
    let closed_again = f.apply(Input::WindowClosed(base_time()));

    assert!(has_reopen(&failed));
    assert!(!has_quit(&failed));
    assert!(has_quit(&closed_again));
}

#[rstest::rstest]
#[case::read_shows_it_in_place(Section::Read, false, 0)]
#[case::export_needs_a_toast(Section::Export, false, 1)]
#[case::send_needs_a_toast(Section::Send, false, 1)]
#[case::closed_on_read_reopens_on_it(Section::Read, true, 0)]
#[case::closed_on_export_reopens_on_it(Section::Export, true, 1)]
fn a_failed_run_toasts_only_where_read_is_not_the_section(
    #[case] section: Section,
    #[case] closed: bool,
    #[case] toasts: usize,
) {
    let mut f = fixture();
    let (id, _) = f.connected();
    f.state.section = section;
    if closed {
        f.apply(Input::WindowClosed(base_time()));
    }

    let out = f.apply(done(
        id,
        Ok(summary(RunEnd::Stopped, &["0"])),
        clean_finish(),
    ));

    assert_eq!(has_reopen(&out), closed);
    assert_eq!(f.failure().stage, FailureStage::Run);
    assert_eq!(f.state.toasts().entries().len(), toasts);
}

#[test]
fn a_session_still_finishing_after_thirty_seconds_reopens_the_window() {
    let mut f = fixture();
    f.connected();
    let closed = base_time();
    f.apply(Input::WindowClosed(closed));

    let early = f.apply(Input::Tick(closed + TimeDelta::seconds(29)));
    let late = f.apply(Input::Tick(closed + TimeDelta::seconds(31)));

    assert!(!has_reopen(&early));
    assert!(has_reopen(&late));
    assert!(f.state.still_finishing);
    assert!(!f.state.window_is_closed());
}

#[test]
fn closing_again_after_the_still_finishing_reopen_waits_a_fresh_thirty_seconds() {
    let mut f = fixture();
    f.connected();
    let first = base_time();
    f.apply(Input::WindowClosed(first));
    f.apply(Input::Tick(first + TimeDelta::seconds(31)));
    let second = first + TimeDelta::seconds(40);
    f.apply(Input::WindowClosed(second));

    let early = f.apply(Input::Tick(second + TimeDelta::seconds(29)));
    let late = f.apply(Input::Tick(second + TimeDelta::seconds(31)));

    assert!(!has_reopen(&early));
    assert!(has_reopen(&late));
}

#[test]
fn force_quit_quits_once_the_reopened_window_offers_it() {
    let mut f = fixture();
    f.connected();
    let closed = base_time();
    f.apply(Input::WindowClosed(closed));
    f.apply(Input::Tick(closed + TimeDelta::seconds(31)));

    let out = f.apply(Input::ForceQuit);

    assert!(has_quit(&out));
}

#[test]
fn force_quit_is_ignored_when_no_session_is_still_finishing() {
    let mut f = fixture();
    f.connected();

    let out = f.apply(Input::ForceQuit);

    assert!(!has_quit(&out));
    assert!(matches!(
        f.state.connection,
        ConnectionState::Connected { .. }
    ));
}

#[test]
fn the_still_finishing_reopen_shows_the_read_section() {
    let mut f = fixture();
    f.connected();
    f.apply(Input::SectionPicked(Section::Export));
    let closed = base_time();
    f.apply(Input::WindowClosed(closed));

    f.apply(Input::Tick(closed + TimeDelta::seconds(31)));

    assert_eq!(f.state.section, Section::Read);
}

#[test]
fn a_data_folder_change_is_refused_while_a_session_starts_or_runs() {
    let mut f = fixture();
    let before = f.state.form.data_folder.clone();
    f.preparing();
    let preparing = f.apply(Input::DataFolderPicked("other".into()));
    let mut g = fixture();
    g.with_session();
    let starting = g.apply(Input::DataFolderPicked("other".into()));
    let mut h = fixture();
    h.connected();
    let connected = h.apply(Input::DataFolderPicked("other".into()));
    let mut i = fixture();
    i.stopping();
    let stopping = i.apply(Input::DataFolderPicked("other".into()));

    for out in [&preparing, &starting, &connected, &stopping] {
        assert!(out.effects.is_empty());
    }
    assert_eq!(f.state.form.data_folder, before);
    assert_eq!(i.state.form.data_folder, before);
}

#[test]
fn a_data_folder_change_while_idle_switches_the_store() {
    let mut f = fixture();
    f.state.connection = ConnectionState::Failed(Failure::starting("earlier"));

    let out = f.apply(Input::DataFolderPicked("captures".into()));

    let [Effect::SwitchDataFolder(paths)] = out.effects.as_slice() else {
        panic!("expected SwitchDataFolder, got {:?}", out.effects);
    };
    assert_eq!(paths, &DataPaths::new("captures"));
    assert_eq!(f.state.form.data_folder, "captures");
}

#[test]
fn save_runs_once_at_a_time_and_success_marks_the_secret_stored() {
    let mut f = fixture();

    let first = f.apply(Input::SavePressed);
    let second = f.apply(Input::SavePressed);
    f.apply(Input::ConfigSaved(Ok(())));

    assert!(matches!(first.effects.as_slice(), [Effect::SaveConfig(_)]));
    assert!(second.effects.is_empty());
    assert_eq!(f.state.save_status, SaveStatus::Saved);
    assert!(f.state.form.connection_string.is_stored());
    assert!(typed_text(&f.state).is_empty());
}

#[test]
fn connect_after_a_successful_save_uses_the_stored_secret() {
    let mut f = fixture();
    f.apply(Input::SavePressed);
    f.apply(Input::ConfigSaved(Ok(())));

    let out = f.apply(Input::ConnectPressed);

    let [Effect::PrepareSession { request, .. }] = out.effects.as_slice() else {
        panic!("expected PrepareSession, got {:?}", out.effects);
    };
    assert!(request.given.connection_string.is_none());
}

#[test]
fn a_secret_typed_while_saving_stays_typed() {
    let mut f = fixture();
    f.apply(Input::SavePressed);
    f.state
        .form
        .connection_string
        .edit_mut()
        .replace_text("typed-after-save");

    f.apply(Input::ConfigSaved(Ok(())));

    assert!(!f.state.form.connection_string.is_stored());
    assert_eq!(typed_text(&f.state), "typed-after-save");
}

#[test]
fn a_save_that_drops_a_stored_secret_turns_it_back_to_typed() {
    let mut f = fixture();
    f.state.form.connection_string = SecretSource::stored();
    f.state.form.set_auth(crate::form::AuthChoice::Credential);
    f.state
        .form
        .namespace
        .replace_text("orders.servicebus.windows.net");
    f.state.form.event_hub.replace_text("orders");
    f.apply(Input::SavePressed);

    f.apply(Input::ConfigSaved(Ok(())));

    assert!(!f.state.form.connection_string.is_stored());
}

#[test]
fn a_data_folder_change_forgets_stored_secrets() {
    let mut f = fixture();
    f.state.form.connection_string = SecretSource::stored();

    f.apply(Input::DataFolderPicked("other".into()));

    assert!(!f.state.form.connection_string.is_stored());
}

#[test]
fn a_data_folder_change_is_refused_while_saving() {
    let mut f = fixture();
    let before = f.state.form.data_folder.clone();
    f.apply(Input::SavePressed);

    let out = f.apply(Input::DataFolderPicked("other".into()));

    assert!(out.effects.is_empty());
    assert_eq!(f.state.form.data_folder, before);
}

#[test]
fn the_quit_is_asked_for_once() {
    let mut f = fixture();

    let closed = f.apply(Input::WindowClosed(base_time()));
    let next_tick = f.apply(Input::Tick(base_time()));

    assert!(has_quit(&closed));
    assert!(!has_quit(&next_tick));
}

#[test]
fn closing_the_window_during_a_save_quits_once_the_save_lands() {
    let mut f = fixture();
    f.apply(Input::SavePressed);

    let closed = f.apply(Input::WindowClosed(base_time()));
    let saved = f.apply(Input::ConfigSaved(Ok(())));

    assert!(!has_quit(&closed));
    assert!(has_quit(&saved));
}

#[test]
fn connect_during_a_save_prepares_a_session() {
    let mut f = fixture();
    f.apply(Input::SavePressed);

    let out = f.apply(Input::ConnectPressed);

    assert!(matches!(
        out.effects.as_slice(),
        [Effect::PrepareSession { .. }]
    ));
}

#[test]
fn a_failure_after_a_run_is_a_run_failure() {
    let mut f = fixture();
    let id = f.stopping();

    f.apply(done(
        id,
        Ok(summary(RunEnd::Stopped, &[])),
        Err("the archive could not be read".to_owned()),
    ));

    assert_eq!(f.failure().stage, FailureStage::Run);
}

#[test]
fn a_failed_save_keeps_the_secret_and_shows_why() {
    let mut f = fixture();
    f.apply(Input::SavePressed);

    f.apply(Input::ConfigSaved(
        Err("belongs to another tool".to_owned()),
    ));

    assert_eq!(
        f.state.save_status,
        SaveStatus::Failed("belongs to another tool".to_owned())
    );
    assert_eq!(
        f.state.toasts().entries()[0].options.kind,
        ToastKind::Danger
    );
    assert_eq!(typed_text(&f.state), SECRET);
}

#[test]
fn closing_the_sheet_hides_a_revealed_secret() {
    let mut f = fixture();
    f.apply(Input::OpenSheet(SheetPurpose::Connect));
    f.state.form.connection_string.edit_mut().set_revealed(true);

    f.apply(Input::CloseSheet);

    assert!(f.state.sheet.is_none());
    assert!(!f.state.form.connection_string.edit().is_revealed());
}

#[test]
fn run_effects_without_services_reports_an_internal_error() {
    let mut f = fixture();
    let out = f.apply(Input::ConnectPressed);

    crate::app::run_effects(&mut f.state, out.effects);

    assert_eq!(f.state.toasts().entries().len(), 1);
    assert!(
        f.state.toasts().entries()[0]
            .options
            .text
            .starts_with("Internal error")
    );
}

fn stats_delivery(session: u64, received: u64, stored: u64) -> Input {
    Input::Delivery(UiDelivery {
        session,
        at: Utc::now(),
        events: Vec::new(),
        stats: Some(StatsSnapshot {
            received,
            stored,
            ..StatsSnapshot::default()
        }),
    })
}

fn shown_counts(state: &AppState) -> Option<(u64, u64)> {
    state
        .read
        .shown
        .as_ref()
        .map(|shown| (shown.stats.received, shown.stats.stored))
}

#[test]
fn delivered_statistics_wait_for_the_next_tick() {
    let mut f = fixture();
    let (id, _) = f.connected();

    let delivered = f.apply(stats_delivery(id, 12, 11));

    assert!(!delivered.changed);
    assert_eq!(shown_counts(&f.state), None);
}

#[test]
fn a_tick_shows_new_counts_once_and_animates_them() {
    let mut f = fixture();
    let (id, _) = f.connected();
    f.apply(stats_delivery(id, 12_403, 12_380));
    let now = base_time();

    let first = f.apply(Input::Tick(now));
    let second = f.apply(Input::Tick(now));

    assert!(first.changed && first.motion == Motion::Animated);
    assert!(!second.changed);
    assert_eq!(shown_counts(&f.state), Some((12_403, 12_380)));
}

#[test]
fn statistics_before_the_session_connects_show_nothing() {
    let mut f = fixture();
    let (id, _) = f.with_session();
    f.apply(stats_delivery(id, 3, 3));

    let tick = f.apply(Input::Tick(base_time()));

    assert!(!tick.changed);
    assert_eq!(shown_counts(&f.state), None);
}

#[test]
fn a_new_session_starts_with_no_statistics() {
    let mut f = fixture();
    let (id, _) = f.connected();
    f.apply(stats_delivery(id, 7, 7));
    f.apply(Input::Tick(base_time()));
    f.apply(Input::DisconnectPressed);
    f.apply(done(id, Ok(summary(RunEnd::Stopped, &[])), clean_finish()));
    f.state.form.connection_string = typed_secret();

    f.preparing();

    assert_eq!(f.state.read.stats, None);
    assert_eq!(shown_counts(&f.state), None);
}

#[test]
fn another_sessions_statistics_are_ignored() {
    let mut f = fixture();
    let (id, _) = f.connected();

    f.apply(stats_delivery(id + 1, 9, 9));
    f.apply(Input::Tick(base_time()));

    assert_eq!(f.state.read.stats, None);
}

#[test]
fn entering_connected_animates_the_strip_in() {
    let mut f = fixture();
    let (id, _) = f.with_session();

    let out = f.apply(delivery(
        id,
        EngineEvent::Started {
            partitions: vec!["0".to_owned()],
        },
    ));

    assert_eq!(out.motion, Motion::Animated);
}

#[test]
fn an_appearance_change_redraws_only_when_it_differs() {
    let mut f = fixture();

    let first = f.apply(Input::AppearanceChanged(false));
    let same = f.apply(Input::AppearanceChanged(false));
    let flipped = f.apply(Input::AppearanceChanged(true));

    assert_eq!(
        (first.changed, same.changed, flipped.changed),
        (true, false, true)
    );
    assert_eq!(f.state.system_dark, Some(true));
}

fn bacon_rules() -> shared_eventhub_core::filters::CaptureFilters {
    shared_eventhub_core::filters::CaptureFilters {
        time: None,
        text: Some(shared_eventhub_core::filters::TextFilter::Contains(vec![
            "bacon".to_owned(),
        ])),
    }
}

#[test]
fn connect_starts_the_engine_with_the_applied_rules() {
    let mut f = fixture();
    f.state.read.rules_applied = bacon_rules();

    let out = f.apply(Input::ConnectPressed);

    let [Effect::PrepareSession { request, .. }] = out.effects.as_slice() else {
        panic!("expected PrepareSession, got {:?}", out.effects);
    };
    assert_eq!(request.config.capture.filters, bacon_rules());
}

#[test]
fn save_config_writes_the_applied_rules() {
    let mut f = fixture();
    f.state.read.rules_applied = bacon_rules();

    let out = f.apply(Input::SavePressed);

    let [Effect::SaveConfig(request)] = out.effects.as_slice() else {
        panic!("expected SaveConfig, got {:?}", out.effects);
    };
    assert_eq!(request.config.capture.filters, bacon_rules());
}

#[test]
fn applied_rules_reach_the_running_session() {
    let mut f = fixture();
    let (_, mut commands) = f.connected();
    f.state.read.rules = crate::read::rules::RulesForm::from_filters(&bacon_rules());

    let out = f.apply(Input::Read(ReadInput::ApplyRules));
    crate::app::run_effects(&mut f.state, out.effects);

    assert!(matches!(
        commands.try_recv(),
        Ok(EngineCommand::UpdateFilters(filters)) if filters == bacon_rules()
    ));
}

#[test]
fn a_delivered_message_waits_for_the_list_flush() {
    let mut f = fixture();
    let (id, _) = f.connected();

    let delivered = f.apply(delivery(
        id,
        EngineEvent::Message(Box::new(crate::read::test_support::captured("0", 1, "a"))),
    ));
    let flushed = f.apply(Input::Read(ReadInput::ListFlush));

    assert!(!delivered.changed);
    assert!(flushed.changed && flushed.motion == Motion::Instant);
    assert_eq!(f.state.read.rows.displayed().len(), 1);
}

#[test]
fn partition_errors_and_ends_go_to_the_session_log() {
    let mut f = fixture();
    let (id, _) = f.connected();

    f.apply(delivery(
        id,
        EngineEvent::Error {
            partition_id: Some("2".to_owned()),
            error: "link lost".to_owned(),
            fatal: false,
        },
    ));
    f.state.pill_open = true;
    let out = f.apply(delivery(
        id,
        EngineEvent::PartitionEnded {
            partition_id: "2".to_owned(),
            end: PartitionEnd::Failed("link lost".to_owned()),
        },
    ));

    assert!(out.changed);
    assert_eq!(f.state.read.session_log.len(), 2);
    assert_eq!(f.state.read.session_log_unseen, 1);
}

#[test]
fn a_closed_window_waits_for_a_running_export_before_quitting() {
    let mut f = fixture();
    let key = shared_eventhub_core::models::MessageKey {
        partition_id: "0".to_owned(),
        sequence_number: 1,
    };
    f.state.read.inspector.export = crate::read::inspector::ExportPhase::Running {
        owner: 1,
        key: key.clone(),
        key_str: "p0-s1".to_owned(),
    };

    let closed = f.apply(Input::WindowClosed(base_time()));
    let finished = f.apply(Input::inspector(
        ListSection::Read,
        InspectorInput::ExportDone {
            owner: 1,
            key,
            result: Ok(()),
            now: base_time(),
        },
    ));

    assert!(!has_quit(&closed));
    assert!(has_quit(&finished));
}

#[test]
fn a_failed_message_export_shows_a_danger_toast() {
    let mut f = fixture();
    let key = shared_eventhub_core::models::MessageKey {
        partition_id: "0".to_owned(),
        sequence_number: 1,
    };

    f.apply(Input::inspector(
        ListSection::Read,
        InspectorInput::ExportDone {
            owner: 1,
            key,
            result: Err("disk full".to_owned()),
            now: base_time(),
        },
    ));

    assert_eq!(f.state.toasts().entries().len(), 1);
    assert_eq!(
        f.state.toasts().entries()[0].options.kind,
        ToastKind::Danger
    );
    assert!(
        f.state.toasts().entries()[0]
            .options
            .text
            .contains("disk full")
    );
}

impl Fixture {
    /// An empty search of the Export section whose first page showed `count` rows.
    fn searched(&mut self, count: usize) {
        let out = self.apply(Input::Archive(ArchiveInput::SearchPressed));
        let [Effect::Archive(ArchiveEffect::Search(request))] = out.effects.as_slice() else {
            panic!("expected a page request, got {:?}", out.effects);
        };
        let generation = request.generation;
        self.apply(Input::Archive(ArchiveInput::PageLoaded {
            generation,
            elapsed_ms: 1,
            result: Ok(archive_page(archive_messages(count), None)),
        }));
    }

    /// A bulk export of the searched rows started; returns its id.
    fn bulk_running(&mut self) -> u64 {
        self.searched(2);
        let out = self.apply(Input::Archive(ArchiveInput::BulkExportPressed));
        match out.effects.as_slice() {
            [Effect::Archive(ArchiveEffect::BulkExport(request))] => request.export,
            other => panic!("expected a bulk export, got {other:?}"),
        }
    }
}

fn bulk_done(export: u64, result: Result<ExportOutcome, String>) -> Input {
    Input::Archive(ArchiveInput::BulkDone {
        export,
        elapsed_ms: 1,
        result,
    })
}

fn files_failed(cancelled: bool) -> Result<ExportOutcome, String> {
    Ok(ExportOutcome {
        failed: 1,
        first_error: Some("x.txt: disk full".to_owned()),
        cancelled,
        ..ExportOutcome::default()
    })
}

#[rstest::rstest]
#[case::the_bar_shows_it(Section::Export, 0)]
#[case::the_bar_is_hidden(Section::Read, 1)]
fn a_failed_bulk_export_toasts_only_where_its_bar_cannot_be_seen(
    #[case] section: Section,
    #[case] toasts: usize,
) {
    let mut f = fixture();
    let export = f.bulk_running();
    f.state.section = section;

    f.apply(bulk_done(export, files_failed(false)));

    assert_eq!(f.state.toasts().entries().len(), toasts);
    assert!(
        f.state
            .toasts()
            .entries()
            .iter()
            .all(|t| t.options.kind == ToastKind::Danger && t.options.text.contains("disk full"))
    );
}

#[test]
fn the_data_folder_cannot_change_while_a_bulk_export_runs() {
    let mut f = fixture();
    let export = f.bulk_running();
    let before = f.state.form.data_folder.clone();

    let refused = f.apply(Input::DataFolderPicked(PathBuf::from("other")));
    let refused_folder = f.state.form.data_folder.clone();
    f.apply(bulk_done(export, clean_finish()));
    let accepted = f.apply(Input::DataFolderPicked(PathBuf::from("other")));

    assert!(!refused.changed && refused_folder == before);
    assert!(matches!(
        accepted.effects.as_slice(),
        [Effect::SwitchDataFolder(_)]
    ));
    assert!(!f.state.archive.searched());
}

#[test]
fn the_data_folder_lock_names_what_holds_the_folder() {
    let mut f = fixture();
    let free = f.state.data_folder_lock();
    f.bulk_running();
    let exporting = f.state.data_folder_lock();
    f.state.archive.bulk = crate::archive::bulk::BulkExport::Idle;
    f.connected();
    let connected = f.state.data_folder_lock();

    assert!(free.is_none());
    assert!(exporting.is_some_and(|reason| reason.contains("bulk export")));
    assert!(connected.is_some_and(|reason| reason.contains("session")));
}

const MESSAGE_EXPORT_LOCK: &str = "The data folder can change once the message export ends.";

impl Fixture {
    fn export_phase(&self, section: ListSection) -> &ExportPhase {
        match section {
            ListSection::Read => &self.state.read.inspector.export,
            ListSection::Export => &self.state.archive.inspector.export,
        }
    }

    /// Export pressed on `section`'s selected message and a file picked; returns the export's
    /// owner and key.
    fn export_selected(&mut self, section: ListSection) -> (u64, MessageKey) {
        self.apply(Input::inspector(section, InspectorInput::ExportPressed));
        let (owner, key) = match self.export_phase(section) {
            ExportPhase::Picking { owner, key, .. } => (*owner, key.clone()),
            other => panic!("expected the save dialog, got {other:?}"),
        };
        self.apply(Input::inspector(
            section,
            InspectorInput::ExportAnswered {
                owner,
                key: key.clone(),
                answer: Ok(Some(PathBuf::from("one.txt"))),
            },
        ));
        (owner, key)
    }

    /// A message export from `section`'s inspector still writing while nothing else holds the
    /// data folder (a Read export outlives its session); returns its owner and key.
    fn message_export_running(&mut self, section: ListSection) -> (u64, MessageKey) {
        let export = match section {
            ListSection::Read => {
                let (session, _) = self.connected();
                self.apply(delivery(
                    session,
                    EngineEvent::Message(Box::new(crate::read::test_support::captured(
                        "0", 1, "a",
                    ))),
                ));
                self.apply(Input::Read(ReadInput::ListFlush));
                self.apply(Input::Read(ReadInput::Select("p0-s1".to_owned())));
                let export = self.export_selected(section);
                self.apply(done(
                    session,
                    Ok(summary(RunEnd::Stopped, &[])),
                    clean_finish(),
                ));
                export
            }
            ListSection::Export => {
                self.searched(1);
                self.apply(Input::Archive(ArchiveInput::Select("p0-s0".to_owned())));
                self.export_selected(section)
            }
        };
        assert!(self.export_phase(section).is_running());
        export
    }
}

fn message_export_done(
    section: ListSection,
    owner: u64,
    key: MessageKey,
    result: Result<(), String>,
) -> Input {
    Input::inspector(
        section,
        InspectorInput::ExportDone {
            owner,
            key,
            result,
            now: base_time(),
        },
    )
}

#[rstest::rstest]
#[case::read(ListSection::Read)]
#[case::export(ListSection::Export)]
fn the_data_folder_lock_names_a_running_message_export(#[case] section: ListSection) {
    let mut f = fixture();
    f.message_export_running(section);

    let reason = f.state.data_folder_lock();

    assert_eq!(reason, Some(MESSAGE_EXPORT_LOCK));
}

#[test]
fn an_open_message_export_dialog_leaves_the_data_folder_free() {
    let mut f = fixture();
    f.searched(1);
    f.apply(Input::Archive(ArchiveInput::Select("p0-s0".to_owned())));

    f.apply(Input::inspector(
        ListSection::Export,
        InspectorInput::ExportPressed,
    ));

    assert!(matches!(
        f.state.archive.inspector.export,
        ExportPhase::Picking { .. }
    ));
    assert!(f.state.data_folder_lock().is_none());
}

#[rstest::rstest]
#[case::read(ListSection::Read)]
#[case::export(ListSection::Export)]
fn a_data_folder_pick_is_refused_while_a_message_export_runs(#[case] section: ListSection) {
    let mut f = fixture();
    f.message_export_running(section);
    let folder = f.state.form.data_folder.clone();
    let searched = f.state.archive.searched();

    let out = f.apply(Input::DataFolderPicked(PathBuf::from("other")));

    assert!(!out.changed && out.effects.is_empty());
    assert_eq!(f.state.form.data_folder, folder);
    assert_eq!(f.state.archive.searched(), searched);
}

#[rstest::rstest]
#[case::read_exported(ListSection::Read, Ok(()))]
#[case::read_failed(ListSection::Read, Err("disk full".to_owned()))]
#[case::export_exported(ListSection::Export, Ok(()))]
#[case::export_failed(ListSection::Export, Err("disk full".to_owned()))]
fn only_its_own_export_done_frees_the_data_folder(
    #[case] section: ListSection,
    #[case] result: Result<(), String>,
) {
    let mut f = fixture();
    let (owner, key) = f.message_export_running(section);

    f.apply(message_export_done(
        section,
        owner.wrapping_add(1),
        key.clone(),
        Ok(()),
    ));
    let after_another = f.state.data_folder_lock();
    f.apply(message_export_done(section, owner, key, result));
    let picked = f.apply(Input::DataFolderPicked(PathBuf::from("other")));

    assert_eq!(after_another, Some(MESSAGE_EXPORT_LOCK));
    assert!(matches!(
        picked.effects.as_slice(),
        [Effect::SwitchDataFolder(_)]
    ));
}

#[test]
fn a_bulk_export_is_named_before_a_message_export() {
    let mut f = fixture();
    f.bulk_running();
    f.apply(Input::Archive(ArchiveInput::Select("p0-s0".to_owned())));
    f.export_selected(ListSection::Export);

    let reason = f.state.data_folder_lock();

    assert!(f.state.archive.inspector.export.is_running());
    assert_eq!(
        reason,
        Some("The data folder can change once the bulk export ends.")
    );
}

#[test]
fn a_closed_window_cancels_a_bulk_export_and_quits_after_it_ends() {
    let mut f = fixture();
    let export = f.bulk_running();

    let closed = f.apply(Input::WindowClosed(base_time()));
    let cancelled = match &f.state.archive.bulk {
        crate::archive::bulk::BulkExport::Running { cancel, .. } => {
            cancel.load(std::sync::atomic::Ordering::Acquire)
        }
        other => panic!("expected Running, got {other:?}"),
    };
    let finished = f.apply(bulk_done(
        export,
        Ok(ExportOutcome {
            cancelled: true,
            ..ExportOutcome::default()
        }),
    ));

    assert!(cancelled);
    assert!(!has_quit(&closed));
    assert!(has_quit(&finished) && !has_reopen(&finished));
}

#[rstest::rstest]
#[case::export_first(true)]
#[case::session_first(false)]
fn a_closed_window_quits_only_after_both_the_session_and_the_export_end(
    #[case] export_first: bool,
) {
    let mut f = fixture();
    let export = f.bulk_running();
    let (id, _) = f.connected();
    f.apply(Input::WindowClosed(base_time()));
    let end_session = || done(id, Ok(summary(RunEnd::Stopped, &[])), clean_finish());
    let end_export = || bulk_done(export, clean_finish());

    let (first, second) = if export_first {
        (f.apply(end_export()), f.apply(end_session()))
    } else {
        (f.apply(end_session()), f.apply(end_export()))
    };

    assert!(!has_quit(&first));
    assert!(has_quit(&second));
}

#[rstest::rstest]
#[case::failed_files(files_failed(false))]
#[case::cancelled_with_failed_files(files_failed(true))]
#[case::stopped(Err("read failed".to_owned()))]
fn a_failed_bulk_export_with_the_window_closed_reopens_it(
    #[case] result: Result<ExportOutcome, String>,
) {
    let mut f = fixture();
    let export = f.bulk_running();
    f.apply(Input::WindowClosed(base_time()));

    let finished = f.apply(bulk_done(export, result));

    assert!(has_reopen(&finished));
    assert!(!has_quit(&finished));
    assert_eq!(f.state.toasts().entries().len(), 1);
}

#[test]
fn a_bulk_export_failing_with_the_window_closed_on_export_reopens_to_its_bar_without_a_toast() {
    let mut f = fixture();
    let export = f.bulk_running();
    f.state.section = Section::Export;
    f.apply(Input::WindowClosed(base_time()));

    let finished = f.apply(bulk_done(export, files_failed(false)));

    assert!(has_reopen(&finished));
    assert!(f.state.toasts().entries().is_empty());
    assert!(matches!(f.state.archive.bulk, BulkExport::Done { .. }));
}

#[test]
fn a_session_failing_first_reopens_once_and_the_export_ending_later_does_not_quit() {
    let mut f = fixture();
    let export = f.bulk_running();
    let (id, _) = f.connected();
    f.apply(Input::WindowClosed(base_time()));

    let failed = f.apply(done(
        id,
        Ok(summary(RunEnd::Stopped, &["0"])),
        clean_finish(),
    ));
    let finished = f.apply(bulk_done(export, files_failed(false)));

    assert!(has_reopen(&failed));
    assert!(!has_reopen(&finished) && !has_quit(&finished));
}

#[test]
fn a_closed_window_waits_for_an_export_section_message_export() {
    let mut f = fixture();
    let key = shared_eventhub_core::models::MessageKey {
        partition_id: "0".to_owned(),
        sequence_number: 1,
    };
    f.state.archive.inspector.export = crate::read::inspector::ExportPhase::Running {
        owner: 0,
        key: key.clone(),
        key_str: "p0-s1".to_owned(),
    };

    let closed = f.apply(Input::WindowClosed(base_time()));
    let finished = f.apply(Input::inspector(
        ListSection::Export,
        InspectorInput::ExportDone {
            owner: 0,
            key,
            result: Ok(()),
            now: base_time(),
        },
    ));

    assert!(!has_quit(&closed));
    assert!(has_quit(&finished));
}

#[test]
fn an_export_section_result_never_ends_a_read_export_with_the_same_owner() {
    let mut f = fixture();
    let key = shared_eventhub_core::models::MessageKey {
        partition_id: "0".to_owned(),
        sequence_number: 1,
    };
    f.state.read.inspector.export = crate::read::inspector::ExportPhase::Running {
        owner: 1,
        key: key.clone(),
        key_str: "p0-s1".to_owned(),
    };

    f.apply(Input::inspector(
        ListSection::Export,
        InspectorInput::ExportDone {
            owner: 1,
            key,
            result: Ok(()),
            now: base_time(),
        },
    ));

    assert!(f.state.read.export_running());
}

#[test]
fn a_tick_ends_export_section_feedback() {
    let mut f = fixture();
    f.searched(1);
    f.apply(Input::Archive(ArchiveInput::Select("p0-s0".to_owned())));
    let now = base_time();
    f.apply(Input::inspector(
        ListSection::Export,
        InspectorInput::Copied {
            kind: crate::read::inspector::FeedbackKind::CopiedBody,
            now,
        },
    ));

    let tick = f.apply(Input::Tick(now + crate::read::inspector::FEEDBACK_FOR));

    assert!(tick.changed);
    assert!(f.state.archive.inspector.feedback.is_none());
}

impl Fixture {
    /// The form holds a namespace-scoped connection string and names the hub `orders`, so a
    /// Send press can start.
    fn with_hub(&mut self) {
        self.state
            .form
            .connection_string
            .edit_mut()
            .replace_text(SEND_STRING);
        self.state.form.event_hub.replace_text("orders");
        self.state.send.refresh_destination(&self.state.form);
    }

    /// Send pressed in the Send section; returns the run.
    fn send_preparing(&mut self) -> u64 {
        self.with_hub();
        self.state.section = Section::Send;
        let out = self.apply(Input::Send(SendInput::SendPressed));
        match out.effects.as_slice() {
            [Effect::Send(crate::send::SendEffect::StartSend(request))] => request.run,
            other => panic!("expected StartSend, got {other:?}"),
        }
    }

    /// Send pressed and the producer open; returns the run.
    fn send_running(&mut self) -> u64 {
        let run = self.send_preparing();
        self.apply(Input::Send(SendInput::Started {
            run,
            target: crate::send::run::Names {
                namespace: "orders.servicebus.windows.net".to_owned(),
                event_hub: "orders".to_owned(),
            },
            at: base_time(),
        }));
        run
    }
}

const SEND_STRING: &str = "Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKeyName=send;SharedAccessKey=c2VjcmV0";

fn send_finished(run: u64, end: shared_eventhub_core::producer::SendEnd) -> Input {
    Input::Send(SendInput::Finished {
        run,
        summary: shared_eventhub_core::producer::SendSummary {
            sent: 1,
            batches: 1,
            elapsed: Duration::from_secs(1),
            end,
        },
        close_error: None,
    })
}

#[test]
fn a_closed_window_cancels_a_send_and_quits_once_it_ends() {
    let mut f = fixture();
    let run = f.send_running();

    let closed = f.apply(Input::WindowClosed(base_time()));
    let requested = f.state.send.run.cancel_requested();
    let finished = f.apply(send_finished(
        run,
        shared_eventhub_core::producer::SendEnd::Cancelled,
    ));

    assert!(requested);
    assert!(!has_quit(&closed));
    assert!(has_quit(&finished));
}

#[rstest::rstest]
#[case::send_first(true)]
#[case::session_first(false)]
fn a_closed_window_with_a_send_and_a_session_quits_after_both(#[case] send_first: bool) {
    let mut f = fixture();
    let run = f.send_running();
    let (session, _) = f.connected();
    f.apply(Input::WindowClosed(base_time()));
    let send_end = || send_finished(run, shared_eventhub_core::producer::SendEnd::Cancelled);
    let session_end = || done(session, Ok(summary(RunEnd::Stopped, &[])), clean_finish());

    let (first, second) = if send_first {
        (f.apply(send_end()), f.apply(session_end()))
    } else {
        (f.apply(session_end()), f.apply(send_end()))
    };

    assert!(!has_quit(&first));
    assert!(has_quit(&second));
}

#[test]
fn a_failed_send_with_the_window_closed_reopens_it_with_a_danger_toast() {
    let mut f = fixture();
    let run = f.send_running();
    f.apply(Input::WindowClosed(base_time()));

    let out = f.apply(send_finished(
        run,
        shared_eventhub_core::producer::SendEnd::Failed("unauthorized".to_owned()),
    ));

    assert!(has_reopen(&out) && !has_quit(&out));
    assert_eq!(
        f.state
            .toasts()
            .entries()
            .last()
            .map(|toast| toast.options.kind),
        Some(ToastKind::Danger)
    );
}

#[test]
fn a_completed_send_with_the_window_closed_lets_the_quit_proceed() {
    let mut f = fixture();
    let run = f.send_running();
    f.apply(Input::WindowClosed(base_time()));

    let out = f.apply(send_finished(
        run,
        shared_eventhub_core::producer::SendEnd::Completed,
    ));

    assert!(has_quit(&out) && !has_reopen(&out));
}

#[test]
fn the_data_folder_lock_names_a_running_send() {
    let mut f = fixture();
    f.send_running();

    let reason = f.state.data_folder_lock();
    let out = f.apply(Input::DataFolderPicked(PathBuf::from("elsewhere")));

    assert!(reason.is_some_and(|reason| reason.contains("send")));
    assert!(out.effects.is_empty() && !out.changed);
}

#[rstest::rstest]
#[case::connect(SheetPurpose::Connect)]
#[case::send_target(SheetPurpose::SendTarget)]
fn no_sheet_opens_while_a_send_runs(#[case] purpose: SheetPurpose) {
    let mut f = fixture();
    f.send_preparing();

    f.apply(Input::OpenSheet(purpose));

    assert!(f.state.sheet.is_none());
}

#[test]
fn the_send_target_sheet_does_not_open_during_a_session() {
    let mut f = fixture();
    f.connected();

    f.apply(Input::OpenSheet(SheetPurpose::SendTarget));

    assert!(f.state.sheet.is_none());
}

#[test]
fn connect_does_nothing_while_the_send_target_sheet_is_open() {
    let mut f = fixture();
    f.apply(Input::OpenSheet(SheetPurpose::SendTarget));

    let out = f.apply(Input::ConnectPressed);

    assert!(out.effects.is_empty());
    assert!(matches!(f.state.connection, ConnectionState::Disconnected));
}

#[test]
fn a_form_edited_during_a_send_leaves_the_runs_destination() {
    let mut f = fixture();
    f.send_preparing();

    f.state.form.event_hub.replace_text("payments");

    assert!(matches!(
        &f.state.send.run,
        crate::send::run::SendRun::Preparing { target: Some(target), .. } if target.event_hub == "orders"
    ));
}

#[test]
fn a_hub_typed_in_the_sheet_shows_once_the_sheet_closes() {
    let mut f = fixture();
    f.apply(Input::OpenSheet(SheetPurpose::SendTarget));
    f.state
        .form
        .connection_string
        .edit_mut()
        .replace_text(SEND_STRING);
    f.state.form.event_hub.replace_text("orders");
    let before = f.state.send.destination.clone();

    f.apply(Input::CloseSheet);

    assert_eq!(before, crate::send::Destination::Invalid);
    assert_eq!(
        f.state
            .send
            .destination
            .names()
            .map(|names| names.event_hub.as_str()),
        Some("orders")
    );
}

#[test]
fn force_quit_cancels_a_running_send_without_waiting_for_it() {
    let mut f = fixture();
    f.with_hub();
    let session = f.stopping();
    f.apply(Input::WindowClosed(base_time()));
    f.apply(Input::Tick(
        base_time() + STILL_FINISHING_AFTER + TimeDelta::seconds(1),
    ));
    f.send_running();

    let out = f.apply(Input::ForceQuit);

    assert!(f.state.send.run.cancel_requested());
    assert!(has_quit(&out));
    assert!(f.state.connection.session().map(EngineSession::id) == Some(session));
}

#[test]
fn a_save_that_stores_a_typed_string_moves_the_destination_to_stored() {
    let mut f = fixture();
    f.with_hub();
    f.apply(Input::SavePressed);

    f.apply(Input::ConfigSaved(Ok(())));

    assert_eq!(f.state.send.destination, crate::send::Destination::Stored);
}

#[test]
fn a_new_data_folder_forgets_a_stored_secret_and_its_destination() {
    let mut f = fixture();
    f.state.form.connection_string = SecretSource::stored();
    f.state.send.refresh_destination(&f.state.form);
    let before = f.state.send.destination.clone();

    f.apply(Input::DataFolderPicked(PathBuf::from("elsewhere")));

    assert_eq!(before, crate::send::Destination::Stored);
    assert_eq!(f.state.send.destination, crate::send::Destination::Missing);
}

#[test]
fn a_session_start_that_clears_a_typed_string_leaves_no_destination() {
    let mut f = fixture();
    f.with_hub();

    f.with_session();

    assert_eq!(f.state.send.destination, crate::send::Destination::Missing);
}

#[rstest::rstest]
#[case::send_shows(Section::Send, 0)]
#[case::read_shows(Section::Read, 1)]
fn a_send_outcome_toasts_only_when_its_row_cannot_be_seen(
    #[case] section: Section,
    #[case] toasts: usize,
) {
    let mut f = fixture();
    let run = f.send_running();
    f.state.section = section;

    f.apply(send_finished(
        run,
        shared_eventhub_core::producer::SendEnd::Completed,
    ));

    assert_eq!(f.state.toasts().entries().len(), toasts);
}
