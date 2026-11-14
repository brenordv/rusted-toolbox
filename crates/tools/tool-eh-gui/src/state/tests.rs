use super::*;
use crate::bridge::test_support::detached_session;
use crate::form::SecretSource;
use shared_eventhub_core::engine::{EngineCommand, PartitionEnd};
use shared_eventhub_core::stats::StatsTracker;
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
    assert_eq!(request.paths, f.state.form.data_paths());
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

#[test]
fn a_session_that_cannot_start_fails_with_its_cause_and_a_danger_toast() {
    let mut f = fixture();
    let attempt = f.preparing();

    f.apply(Input::SessionReady {
        attempt,
        result: Err("The archive is in use by another eh or eh-gui".to_owned()),
    });

    assert_eq!(
        f.failed_text(),
        "The archive is in use by another eh or eh-gui"
    );
    assert_eq!(f.failure().stage, FailureStage::Starting);
    assert_eq!(f.state.toasts.len(), 1);
    assert_eq!(f.state.toasts[0].kind, ToastKind::Danger);
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
    assert_eq!(f.state.toasts[0].kind, ToastKind::Info);
}

#[test]
fn a_clean_stop_disconnects_without_a_toast() {
    let mut f = fixture();
    let id = f.stopping();

    f.apply(done(id, Ok(summary(RunEnd::Stopped, &[])), clean_finish()));

    assert!(matches!(f.state.connection, ConnectionState::Disconnected));
    assert!(f.state.toasts.is_empty());
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
fn info_toasts_expire_after_their_ticks_and_danger_toasts_stay() {
    let mut f = fixture();
    f.state.toast(ToastKind::Info, "done".to_owned());
    f.state.toast(ToastKind::Danger, "broken".to_owned());

    for second in 0..INFO_TOAST_TICKS {
        f.apply(Input::Tick(
            base_time() + TimeDelta::seconds(i64::from(second)),
        ));
    }

    assert_eq!(f.state.toasts.len(), 1);
    assert_eq!(f.state.toasts[0].kind, ToastKind::Danger);
}

#[test]
fn toasts_past_the_limit_drop_the_oldest() {
    let mut f = fixture();

    for index in 0..6 {
        f.state.toast(ToastKind::Danger, index.to_string());
    }

    let texts: Vec<&str> = f.state.toasts.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(texts, ["2", "3", "4", "5"]);
}

#[test]
fn a_dismissed_toast_leaves() {
    let mut f = fixture();
    f.state.toast(ToastKind::Danger, "broken".to_owned());
    let id = f.state.toasts[0].id;

    f.apply(Input::DismissToast(id));

    assert!(f.state.toasts.is_empty());
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
fn connect_is_refused_while_a_save_runs() {
    let mut f = fixture();
    f.apply(Input::SavePressed);

    let out = f.apply(Input::ConnectPressed);

    assert!(out.effects.is_empty());
    assert!(matches!(f.state.connection, ConnectionState::Disconnected));
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
    assert_eq!(f.state.toasts[0].kind, ToastKind::Danger);
    assert_eq!(typed_text(&f.state), SECRET);
}

#[test]
fn closing_the_sheet_hides_a_revealed_secret() {
    let mut f = fixture();
    f.apply(Input::OpenSheet);
    f.state.form.connection_string.edit_mut().set_revealed(true);

    f.apply(Input::CloseSheet);

    assert!(!f.state.sheet_open);
    assert!(!f.state.form.connection_string.edit().is_revealed());
}

#[test]
fn run_effects_without_services_reports_an_internal_error() {
    let mut f = fixture();
    let out = f.apply(Input::ConnectPressed);

    crate::app::run_effects(&mut f.state, out.effects);

    assert_eq!(f.state.toasts.len(), 1);
    assert!(f.state.toasts[0].text.starts_with("Internal error"));
}
