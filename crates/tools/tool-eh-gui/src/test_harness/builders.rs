//! App states the journeys, sweeps, sequences, and view tests start from.

use super::services::{CallLog, RecordingServices};
use crate::archive::results::Results;
use crate::archive::test_support::{cursor, messages, page};
use crate::archive::{ArchiveEffect, ArchiveInput};
use crate::bridge::UiDelivery;
use crate::bridge::test_support::detached_session;
use crate::form::{AuthChoice, ConnectForm};
use crate::read::ReadInput;
use crate::read::test_support::captured;
use crate::send::compose::PacingChoice;
use crate::send::run::Names;
use crate::send::{SendEffect, SendInput};
use crate::state::{AppState, Effect, Input, Section, SheetPurpose};
use common_cli::app_logger::FileLogging;
use shared_eventhub_core::engine::{EngineEvent, RunEnd, RunSummary};
use shared_eventhub_core::stats::StatsTracker;
use std::collections::BTreeMap;

/// A typed connection string for the `orders` hub; its key is not a secret.
pub const SEND_CONNECTION_STRING: &str = "Endpoint=sb://orders.servicebus.windows.net/;\
    SharedAccessKeyName=send;SharedAccessKey=c2VjcmV0;EntityPath=orders";
/// A typed listen connection string for the `orders` hub; its key is not a secret.
pub const ORDERS: &str = "Endpoint=sb://orders.servicebus.windows.net/;\
    SharedAccessKeyName=listen;SharedAccessKey=bGlzdGVu;EntityPath=orders";

/// The summary of a capture run that was stopped and found nothing.
pub fn stopped() -> RunSummary {
    RunSummary {
        stats: StatsTracker::new(chrono::Utc::now()).snapshot(),
        partitions: BTreeMap::new(),
        end: RunEnd::Stopped,
    }
}

/// A first-launch state: disconnected, no services.
pub fn state() -> AppState {
    AppState::new(None, ConnectForm::default(), None, FileLogging::Off)
}

/// Gives `state` recording services and returns their log.
pub fn record(state: &mut AppState) -> CallLog {
    let log = CallLog::default();
    state.services = Some(Box::new(RecordingServices::new(log.clone())));
    log
}

/// The connection sheet open on `auth`.
pub fn open_sheet(auth: AuthChoice) -> AppState {
    let mut s = state();
    s.form.set_auth(auth);
    s.sheet = Some(SheetPurpose::Connect);
    s
}

/// A state connected to a session over four partitions; the runtime must outlive it.
pub fn connected(rt: &tokio::runtime::Runtime) -> AppState {
    let mut s = state();
    s.form
        .connection_string
        .edit_mut()
        .replace_text("Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKey=k");
    let out = s.apply(Input::ConnectPressed);
    let [Effect::PrepareSession { attempt, .. }] = out.effects.as_slice() else {
        panic!("expected PrepareSession, got {:?}", out.effects);
    };
    let attempt = *attempt;
    let (session, _) = detached_session(attempt, rt.handle());
    s.apply(Input::SessionReady {
        attempt,
        result: Ok(session),
    });
    s.apply(Input::Delivery(UiDelivery {
        at: chrono::Utc::now(),
        session: attempt,
        events: vec![EngineEvent::Started {
            partitions: ["0", "1", "2", "3"].map(str::to_owned).to_vec(),
        }],
        stats: None,
    }));
    s
}

/// Delivers `count` messages on partition 0 from sequence `first` and flushes them onto the
/// list.
pub fn with_rows(s: &mut AppState, first: usize, count: usize) {
    let session = s.connection.session().map(|session| session.id());
    let Some(session) = session else {
        panic!("with_rows needs a connected state");
    };
    let events = (first..first + count)
        .map(|sequence| EngineEvent::Message(Box::new(captured("0", sequence as i64, "body"))))
        .collect();
    s.apply(Input::Delivery(UiDelivery {
        at: chrono::Utc::now(),
        session,
        events,
        stats: None,
    }));
    s.apply(Input::Read(ReadInput::ListFlush));
}

/// The Export section with a search whose first page showed `count` rows, more to come when
/// `more`; the log sees the calls made after that.
pub fn export_results(count: usize, more: bool) -> (AppState, CallLog) {
    let mut s = state();
    let log = record(&mut s);
    s.section = Section::Export;
    let out = s.apply(Input::Archive(ArchiveInput::SearchPressed));
    let [Effect::Archive(ArchiveEffect::Search(request))] = out.effects.as_slice() else {
        panic!("expected a page request, got {:?}", out.effects);
    };
    let generation = request.generation;
    s.apply(Input::Archive(ArchiveInput::PageLoaded {
        generation,
        elapsed_ms: 1,
        result: Ok(page(messages(count), more.then(cursor))),
    }));
    (s, log)
}

/// The Export section showing `results`.
pub fn export_state(results: Results) -> AppState {
    let mut s = state();
    s.section = Section::Export;
    s.archive.results = results;
    s
}

/// The Send section with a usable destination; the log sees each send started.
pub fn send_state() -> (AppState, CallLog) {
    let mut form = ConnectForm::default();
    form.connection_string
        .edit_mut()
        .replace_text(SEND_CONNECTION_STRING);
    let mut s = AppState::new(None, form, None, FileLogging::Off);
    let log = record(&mut s);
    s.section = Section::Send;
    (s, log)
}

/// Starts a send of 1,000 (paced one a second when `paced`) and moves it to 340 sent, four
/// seconds in; returns its run id.
pub fn send_running(s: &mut AppState, paced: bool) -> u64 {
    if paced {
        s.send.composer.pacing = PacingChoice::Paced;
    }
    s.send.composer.count.replace_text("1000");
    let out = s.apply(Input::Send(SendInput::SendPressed));
    let [Effect::Send(SendEffect::StartSend(request))] = out.effects.as_slice() else {
        panic!("expected StartSend, got {:?}", out.effects);
    };
    let run = request.run;
    let at = chrono::Utc::now();
    s.apply(Input::Send(SendInput::Started {
        run,
        target: Names {
            namespace: "orders.servicebus.windows.net".to_owned(),
            event_hub: "orders".to_owned(),
        },
        at,
    }));
    s.apply(Input::Send(SendInput::Progress { run, sent: 340 }));
    s.apply(Input::Tick(at + chrono::TimeDelta::seconds(4)));
    run
}

/// The Send section with `body` and Template on.
pub fn template_state(body: &str) -> (AppState, CallLog) {
    let (mut s, log) = send_state();
    s.send.composer.body.replace_text(body);
    s.send.composer.set_template(true);
    (s, log)
}
