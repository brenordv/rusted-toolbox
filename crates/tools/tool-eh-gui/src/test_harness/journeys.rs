//! Journeys the unit tests and the secret-hygiene binary both drive. Each asserts its steps as
//! it goes and hands back what a caller checks next.

use super::Harness;
use super::builders::state;
use super::services::ServiceCall;
use crate::bridge::UiDelivery;
use crate::bridge::test_support::detached_session;
use crate::design::tokens::WINDOW_SIZE;
use crate::form::ConnectForm;
use crate::read::ReadInput;
use crate::read::test_support::captured;
use crate::send::run::Names;
use crate::send::{SendInput, SendRequest};
use crate::services::ConnectRequest;
use crate::state::{AppState, Input, Section};
use crate::views::{BODY_KEY, COUNT_KEY};
use common_cli::app_logger::FileLogging;
use rune::Key;
use shared_eventhub_core::connection::target_names;
use shared_eventhub_core::engine::{EngineCommand, EngineEvent};
use shared_eventhub_core::producer::{Pacing, SendEnd, SendSummary};
use shared_eventhub_core::stats::StatsSnapshot;
use shared_eventhub_persistence::config::resolve_target_with;
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
