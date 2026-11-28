//! Journeys the unit tests and the secret-hygiene binary both drive. Each asserts its steps as
//! it goes and hands back what a caller checks next.

use super::Harness;
use super::builders::{BILLING, billing_config, listing, profile, state, with_profiles};
use super::services::ServiceCall;
use crate::bridge::UiDelivery;
use crate::bridge::test_support::detached_session;
use crate::design::tokens::WINDOW_SIZE;
use crate::form::ConnectForm;
use crate::read::ReadInput;
use crate::read::test_support::captured;
use crate::send::run::Names;
use crate::send::{SendInput, SendRequest};
use crate::services::{ConnectRequest, CreateRequest, SaveRequest};
use crate::state::{AppState, Input, Section};
use crate::views::{
    BODY_KEY, COUNT_KEY, NAME_ISSUE_KEY, NEW_PROFILE_KEY, NEW_PROFILE_NAME_KEY,
    NEW_PROFILE_SAVE_KEY, PICKER_KEY, PROFILE_GROUP_KEY,
};
use common_cli::app_logger::FileLogging;
use eh_core::connection::target_names;
use eh_core::engine::{EngineCommand, EngineEvent};
use eh_core::producer::{Pacing, SendEnd, SendSummary};
use eh_core::stats::StatsSnapshot;
use eh_persistence::config::{ConfigLoad, resolve_target_with};
use rune::Key;
use std::time::Duration;
use tokio::sync::mpsc;

/// Messages journey 1 captures; even sequences are orders, odd ones refunds.
pub const CAPTURED: usize = 50;

/// A harness connected and capturing, with the request the app made and the session's commands.
pub struct Capturing {
    pub harness: Harness,
    pub request: ConnectRequest,
    pub commands: mpsc::Receiver<EngineCommand>,
}

/// The single call recorded so far, which must be `pick`'s kind.
fn only_call<T>(h: &mut Harness, pick: impl Fn(ServiceCall) -> Option<T>) -> T {
    let mut calls = h.take_calls();
    assert_eq!(calls.len(), 1, "expected one call, got {calls:?}");
    let Some(call) = calls.pop().and_then(pick) else {
        panic!("the call was not the expected kind");
    };
    call
}

/// The namespace `request` resolves to, without the secrets file or the network.
pub fn namespace_of(request: &ConnectRequest) -> String {
    let target = resolve_target_with(&request.config, request.given.clone(), None)
        .unwrap_or_else(|error| panic!("the request does not resolve: {error:#}"));
    match target_names(&target) {
        Some(Ok((namespace, _))) => namespace,
        Some(Err(error)) => panic!("the request names no target: {error:#}"),
        None => panic!("a Key Vault request names no target before its fetch"),
    }
}

/// Journey 1: connect with `connection_string` from the empty state, start the session, and
/// capture [`CAPTURED`] messages.
pub fn connect_and_capture(connection_string: &str) -> Capturing {
    let mut h = Harness::new(state(), WINDOW_SIZE);
    h.press("empty-connect");
    assert!(h.state().sheet.is_some(), "the sheet opened");
    h.type_into("connection-string", connection_string);
    h.press("sheet-connect");

    let (attempt, request) = only_call(&mut h, |call| match call {
        ServiceCall::PrepareSession { attempt, request } => Some((attempt, request)),
        _ => None,
    });
    assert!(h.state().sheet.is_none(), "the sheet closed");
    assert_eq!(h.label_of("pill").as_deref(), Some("Connection: Preparing"));

    let (session, commands) = detached_session(attempt, h.tokio().handle());
    h.deliver(Input::SessionReady {
        attempt,
        result: Ok(session),
    });
    let at = h.now();
    h.deliver(Input::Delivery(UiDelivery {
        at,
        session: attempt,
        events: vec![EngineEvent::Started {
            partitions: ["0", "1", "2", "3"].map(str::to_owned).to_vec(),
        }],
        stats: None,
    }));
    assert!(h.has_key("stats-strip"), "the Read content mounted");
    assert_eq!(
        h.label_of("pill").as_deref(),
        Some("Connection: Connected · 4 partitions")
    );

    let events = (0..CAPTURED)
        .map(|sequence| {
            let body = match sequence % 2 {
                0 => format!("order {sequence}"),
                _ => format!("refund {sequence}"),
            };
            EngineEvent::Message(Box::new(captured("0", sequence as i64, &body)))
        })
        .collect();
    h.deliver(Input::Delivery(UiDelivery {
        at,
        session: attempt,
        events,
        stats: Some(StatsSnapshot {
            received: CAPTURED as u64,
            stored: CAPTURED as u64,
            ..StatsSnapshot::default()
        }),
    }));
    h.deliver(Input::Read(ReadInput::ListFlush));
    h.tick();
    assert_eq!(h.state().read.rows.displayed().len(), CAPTURED);
    assert!(h.has_key("p0-s0"), "the first row is mounted");
    assert_eq!(h.label_of("stat-received").as_deref(), Some("Received 50"));
    Capturing {
        harness: h,
        request,
        commands,
    }
}

/// Opens the sheet from the toolbar and answers its listing with the test workspace's.
pub fn open_listed_sheet(h: &mut Harness) {
    h.press("connect");
    let request = only_call(h, |call| match call {
        ServiceCall::ListProfiles { request, .. } => Some(request),
        _ => None,
    });
    h.deliver(Input::ProfilesListed {
        request,
        result: Ok(listing()),
    });
    assert!(h.has_key(PROFILE_GROUP_KEY), "the profile group mounted");
}

/// Picks `label` from the open sheet's picker with the pointer; returns the load request.
pub fn pick_with_pointer(h: &mut Harness, label: &str) -> u64 {
    h.press(&format!("{PICKER_KEY}-trigger"));
    h.press_named(&format!("{PICKER_KEY}-content"), label);
    only_call(h, |call| match call {
        ServiceCall::LoadProfile { request, .. } => Some(request),
        _ => None,
    })
}

/// Journey 5: types `connection_string` into the sheet over the test workspace, switches to
/// `billing` (which drops it), and saves there. Returns the harness and the save's request.
pub fn switch_dropping_a_typed_secret(connection_string: &str) -> (Harness, SaveRequest) {
    let mut h = Harness::new(with_profiles(state()), WINDOW_SIZE);
    open_listed_sheet(&mut h);
    h.type_into("connection-string", connection_string);
    let request = pick_with_pointer(&mut h, BILLING);
    h.deliver(Input::ProfileLoaded {
        request,
        result: Ok(ConfigLoad::Loaded(Box::new(billing_config()))),
    });
    assert!(
        h.has_text("The connection string you typed was cleared"),
        "the note says the typed string went"
    );
    let switched = h
        .calls
        .take_where(|call| matches!(call, ServiceCall::SwitchDataFolder(_)));
    assert_eq!(switched.len(), 1, "the switch moved the data folder once");
    h.press("save-config");
    let save = only_call(&mut h, |call| match call {
        ServiceCall::SaveConfig(request) => Some(request),
        _ => None,
    });
    (h, save)
}

/// The profile [`save_as_profile`] saves.
pub const NEW_PROFILE: &str = "orders-new";

/// Journey 6: types `connection_string` into the sheet over the test workspace, opens Save as
/// profile, pastes the string into the profile name and presses Save profile (refused), then
/// saves the form as [`NEW_PROFILE`] and answers the save and its load. Returns the harness and
/// the create's request.
pub fn save_as_profile(connection_string: &str) -> (Harness, CreateRequest) {
    let mut h = Harness::new(with_profiles(state()), WINDOW_SIZE);
    open_listed_sheet(&mut h);
    h.type_into("connection-string", connection_string);
    h.press(NEW_PROFILE_KEY);
    h.type_into(NEW_PROFILE_NAME_KEY, connection_string);
    h.press(NEW_PROFILE_SAVE_KEY);
    assert!(
        h.has_key(NAME_ISSUE_KEY),
        "a connection string is not a profile name"
    );
    h.type_into(NEW_PROFILE_NAME_KEY, NEW_PROFILE);
    h.press(NEW_PROFILE_SAVE_KEY);
    let request = only_call(&mut h, |call| match call {
        ServiceCall::CreateProfile(request) => Some(request),
        _ => None,
    });
    h.deliver(Input::ProfileCreated {
        request: request.request,
        result: Ok(profile(NEW_PROFILE)),
    });
    let load = match h.take_calls().as_slice() {
        [
            ServiceCall::ListProfiles { .. },
            ServiceCall::LoadProfile { request, .. },
        ] => *request,
        other => panic!("expected a ListProfiles and a LoadProfile, got {other:?}"),
    };
    h.deliver(Input::ProfileLoaded {
        request: load,
        result: Ok(ConfigLoad::Loaded(Box::new(request.config.clone()))),
    });
    assert!(
        h.has_text("Saved profile orders-new."),
        "the note says the profile was saved"
    );
    (h, request)
}

/// The names a producer for the `orders` hub reports.
pub fn orders() -> Names {
    Names {
        namespace: "orders.servicebus.windows.net".to_owned(),
        event_hub: "orders".to_owned(),
    }
}

/// Journey 4, batched: with `connection_string` in the form, `mod+3`, a body, a count of 3,
/// and `mod+enter` from the body; the run then connects, sends, and completes.
pub fn send_batched(connection_string: &str) -> (Harness, SendRequest) {
    let mut form = ConnectForm::default();
    form.connection_string
        .edit_mut()
        .replace_text(connection_string);
    let mut h = Harness::new(
        AppState::new(None, form, None, FileLogging::Off),
        WINDOW_SIZE,
    );
    h.command(Key::Command('3'));
    assert_eq!(h.state().section, Section::Send);
    h.type_into(COUNT_KEY, "3");
    h.type_into(BODY_KEY, "{\"order\": 1}");
    h.command(Key::Enter);

    let request = only_call(&mut h, |call| match call {
        ServiceCall::StartSend(request) => Some(request),
        _ => None,
    });
    assert_eq!(request.plan.count(), 3);
    assert_eq!(
        request.plan.pacing().describe(),
        Pacing::batched().describe()
    );
    assert!(h.has_key("send-status-connecting"));

    let run = request.run;
    let at = h.now();
    h.deliver(Input::Send(SendInput::Started {
        run,
        target: orders(),
        at,
    }));
    assert_eq!(
        h.label_of("send-status-sending").as_deref(),
        Some("Sending to orders")
    );
    h.deliver(Input::Send(SendInput::Progress { run, sent: 2 }));
    h.tick();
    h.deliver(Input::Send(SendInput::Finished {
        run,
        summary: SendSummary {
            sent: 3,
            batches: 1,
            elapsed: Duration::from_millis(40),
            end: SendEnd::Completed,
        },
        close_error: None,
    }));
    assert!(h.has_key("send-status-done-completed"));
    (h, request)
}
