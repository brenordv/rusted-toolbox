//! A typed connection string or client secret reaches no log record, toast, `Debug` output,
//! accessibility tree, or failure text, through the journeys and through the real session
//! preparation against an endpoint the engine refuses before opening any connection. This is
//! the only test in its binary, so a global subscriber sees every event of the run.

use eh_gui::bridge::{EngineDone, UiDelivery, UiPort};
use eh_gui::design::tokens::WINDOW_SIZE;
use eh_gui::form::{AuthChoice, CredentialChoice};
use eh_gui::services::{ConnectRequest, prepare_session};
use eh_gui::state::{
    AppState, ConnectionState, ERROR_PREVIEW_CHARS, Input, Section, ToastKind, error_preview,
};
use eh_gui::tasks::StoreSlot;
use eh_gui::test_harness::Harness;
use eh_gui::test_harness::builders::state;
use eh_gui::test_harness::journeys::{
    Capturing, connect_and_capture, send_batched, switch_dropping_a_typed_secret,
};
use eh_gui::test_harness::services::ServiceCall;
use eh_gui::views::details::{COPY_KEY, FAILURE_KEY};
use std::io::Write;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;

const MARKER: &str = "marker-55219";
const CLIENT_MARKER: &str = "marker-77310";

#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for SharedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SharedBuffer {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// What a session posts, carried to the test thread that owns the harness.
enum Posted {
    Delivery(UiDelivery),
    Done(EngineDone),
}

struct ChannelPort(Mutex<mpsc::Sender<Posted>>);

impl UiPort for ChannelPort {
    fn deliver(&self, delivery: UiDelivery, ack: oneshot::Sender<()>) {
        self.0
            .lock()
            .unwrap()
            .send(Posted::Delivery(delivery))
            .unwrap();
        let _ = ack.send(());
    }

    fn done(&self, done: EngineDone) {
        self.0.lock().unwrap().send(Posted::Done(done)).unwrap();
    }
}

fn connection_string(endpoint: &str) -> String {
    format!(
        "Endpoint=sb://{endpoint}/;SharedAccessKeyName=listen;SharedAccessKey={MARKER};\
         EntityPath=orders"
    )
}

/// The request Connect builds from `form` in `h`, still recorded.
fn connect(h: &mut Harness) -> (u64, ConnectRequest) {
    h.deliver(Input::ConnectPressed);
    match h.take_calls().pop() {
        Some(ServiceCall::PrepareSession { attempt, request }) => (attempt, request),
        other => panic!("Connect did not prepare a session: {other:?}"),
    }
}

/// Runs the real session preparation for `request` and applies what the session posts to
/// `h`, as the app does; returns the end's failure text.
fn run_refused(h: &mut Harness, attempt: u64, request: ConnectRequest) -> String {
    let tokio = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let slot = StoreSlot::new(request.paths.clone());
    let (posts, posted) = mpsc::channel();
    let port: Arc<dyn UiPort> = Arc::new(ChannelPort(Mutex::new(posts)));
    let (start, started) = oneshot::channel();
    let session = tokio
        .block_on(prepare_session(
            tokio.handle(),
            attempt,
            request,
            &slot,
            port,
            started,
        ))
        .unwrap();
    h.deliver(Input::SessionReady {
        attempt,
        result: Ok(session),
    });
    start.send(()).unwrap();
    loop {
        match posted.recv_timeout(Duration::from_secs(120)).unwrap() {
            Posted::Delivery(delivery) => h.deliver(Input::Delivery(delivery)),
            Posted::Done(done) => {
                let text = format!("{:?}", done.engine);
                h.deliver(Input::EngineDone(done));
                return text;
            }
        }
    }
}

fn assert_failed_with_a_toast(h: &Harness) {
    assert!(matches!(h.state().connection, ConnectionState::Failed(_)));
    assert!(
        h.state()
            .toasts()
            .entries()
            .iter()
            .any(|toast| toast.options.kind == ToastKind::Danger)
    );
}

/// Everything shown or printed for `h`, for the absence checks.
fn surfaces(h: &Harness) -> String {
    let toasts: Vec<&str> = h
        .state()
        .toasts()
        .entries()
        .iter()
        .map(|t| t.options.text.as_str())
        .collect();
    format!(
        "{toasts:?}\n{:?}\n{}\n{:?}",
        h.state(),
        h.a11y(),
        h.rt.elements()
    )
}

/// Opens the details of the connection's failure from the Read section's empty state and
/// copies them; returns the modal's text and what reached the clipboard.
fn copy_failure_details(h: &mut Harness) -> (String, Option<String>) {
    h.deliver(Input::SectionPicked(Section::Read));
    let failure = match &h.state().connection {
        ConnectionState::Failed(failure) => failure.text.clone(),
        other => panic!("not failed: {other:?}"),
    };
    assert!(
        error_preview(&failure).cut,
        "the failure is short enough to show whole, so nothing offers its details: {failure}"
    );
    h.press(FAILURE_KEY);
    h.press(COPY_KEY);
    let shown = h
        .state()
        .details()
        .map(|details| details.text.clone())
        .unwrap_or_default();
    (shown, h.clipboard_text())
}

/// A state in the Export section, where a connection failure toasts.
fn connecting_from_export(data: &std::path::Path) -> AppState {
    let mut s = state();
    s.section = Section::Export;
    s.form.data_folder = data.display().to_string();
    s
}

fn off_allowlist_with_a_typed_key(data: &std::path::Path) -> (Harness, u64, ConnectRequest) {
    let mut s = connecting_from_export(data);
    s.form
        .connection_string
        .edit_mut()
        .replace_text(connection_string("example.invalid"));
    let mut h = Harness::new(s, WINDOW_SIZE);
    let (attempt, request) = connect(&mut h);
    (h, attempt, request)
}

fn off_allowlist_with_a_typed_client_secret(
    data: &std::path::Path,
) -> (Harness, u64, ConnectRequest) {
    let mut s = connecting_from_export(data);
    s.form.set_auth(AuthChoice::Credential);
    s.form.credential = CredentialChoice::ClientSecret;
    s.form
        .namespace
        .replace_text("orders.servicebus.windows.net");
    s.form.event_hub.replace_text("orders");
    s.form.tenant_id.replace_text("tenant");
    s.form.client_id.replace_text("client");
    s.form.client_secret.edit_mut().replace_text(CLIENT_MARKER);
    let mut h = Harness::new(s, WINDOW_SIZE);
    let (attempt, mut request) = connect(&mut h);
    request.config.target.namespace = Some("example.invalid".to_owned());
    (h, attempt, request)
}

#[test]
fn typed_secrets_reach_no_log_toast_debug_output_or_tree() {
    let logs = SharedBuffer::default();
    let writer = logs.clone();
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish(),
    )
    .unwrap();
    let typed = connection_string("orders.servicebus.windows.net");

    let Capturing {
        harness: captured,
        request: connected,
        ..
    } = connect_and_capture(&typed);
    let (sent, send_request) = send_batched(&typed);
    let (switched, switched_save) = switch_dropping_a_typed_secret(&typed);
    let data = tempfile::tempdir().unwrap();
    let (mut refused, attempt, request) = off_allowlist_with_a_typed_key(&data.path().join("a"));
    let key_reached = request
        .given
        .connection_string
        .as_ref()
        .is_some_and(|value| value.expose().contains(MARKER));
    let failure = run_refused(&mut refused, attempt, request);
    let (mut client, attempt, request) =
        off_allowlist_with_a_typed_client_secret(&data.path().join("b"));
    let secret_reached = request
        .given
        .client_secret
        .as_ref()
        .is_some_and(|value| value.expose().contains(CLIENT_MARKER));
    let client_failure = run_refused(&mut client, attempt, request);
    assert_failed_with_a_toast(&refused);
    assert_failed_with_a_toast(&client);
    let (refused_details, refused_copied) = copy_failure_details(&mut refused);
    let (client_details, client_copied) = copy_failure_details(&mut client);
    let text = logs.text();

    let exposes = |request: &ConnectRequest| {
        request
            .given
            .connection_string
            .as_ref()
            .is_some_and(|value| value.expose().contains(MARKER))
    };
    assert!(text.contains("connecting"), "the UI's records are captured");
    assert!(text.contains("Resolved the target"));
    assert!(
        text.contains("the at-end pass ended"),
        "a session task's records are captured"
    );
    assert!(exposes(&connected) && exposes(&send_request.target));
    assert!(key_reached && secret_reached);
    assert!(switched_save.given.connection_string.is_none());
    assert!(
        switched
            .state()
            .form
            .connection_string
            .edit()
            .text
            .is_empty()
    );
    assert!(
        failure.contains("does not have one of the required suffixes"),
        "{failure}"
    );
    assert!(
        client_failure.contains("does not have one of the required suffixes"),
        "{client_failure}"
    );
    for (shown, copied) in [
        (&refused_details, &refused_copied),
        (&client_details, &client_copied),
    ] {
        assert_eq!(copied.as_deref(), Some(shown.as_str()));
        assert!(shown.chars().count() > ERROR_PREVIEW_CHARS, "{shown}");
    }
    let everything = [
        text,
        failure,
        client_failure,
        refused_details,
        client_details,
        refused_copied.unwrap_or_default(),
        client_copied.unwrap_or_default(),
        format!("{connected:?}\n{send_request:?}\n{switched_save:?}"),
        surfaces(&captured),
        surfaces(&sent),
        surfaces(&switched),
        switched.state().config_caption.clone().unwrap_or_default(),
        surfaces(&refused),
        surfaces(&client),
    ]
    .join("\n");
    assert!(!everything.contains(MARKER));
    assert!(!everything.contains(CLIENT_MARKER));
}
