//! The log lines of a close that waits past its bound name the jobs still running: the reopen's
//! line, and Force quit's line with a session started after the reopen. This is the only test
//! in its binary, so a global subscriber sees every event without other tests' events mixed in.

use chrono::{TimeDelta, Utc};
use common_cli::app_logger::FileLogging;
use eh_gui::bridge::test_support::detached_session;
use eh_gui::form::{ConnectForm, SecretSource};
use eh_gui::state::{AppState, Effect, Input, STILL_FINISHING_AFTER};
use std::io::Write;
use std::sync::{Arc, Mutex};

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

fn line_with<'a>(logs: &'a str, message: &str) -> &'a str {
    logs.lines()
        .find(|line| line.contains(message))
        .unwrap_or_else(|| panic!("no log line holds '{message}':\n{logs}"))
}

#[test]
fn the_overdue_reopen_and_force_quit_log_the_jobs_still_running() {
    let buffer = SharedBuffer::default();
    let writer = buffer.clone();
    tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish(),
    )
    .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut form = ConnectForm::default();
    form.connection_string = SecretSource::typed();
    form.connection_string
        .edit_mut()
        .replace_text("Endpoint=sb://orders.servicebus.windows.net/;SharedAccessKey=marker");
    let mut state = AppState::new(None, form, None, FileLogging::Off);
    state.apply(Input::SavePressed);
    let closed = Utc::now();
    state.apply(Input::WindowClosed(closed));
    state.apply(Input::Tick(
        closed + STILL_FINISHING_AFTER + TimeDelta::seconds(1),
    ));
    let connecting = state.apply(Input::ConnectPressed);
    let [Effect::PrepareSession { attempt, .. }] = connecting.effects.as_slice() else {
        panic!("expected PrepareSession, got {:?}", connecting.effects);
    };
    let (session, _commands) = detached_session(*attempt, rt.handle());
    state.apply(Input::SessionReady {
        attempt: *attempt,
        result: Ok(session),
    });

    let forced = state.apply(Input::ForceQuit);

    let logs = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
    let reopened = line_with(&logs, "still finishing; reopening the window");
    let quit = line_with(&logs, "force quit: stopped waiting for running work");
    assert!(reopened.contains("still_running=[Save]"), "{reopened}");
    assert!(quit.contains("still_running=[Session, Save]"), "{quit}");
    assert!(
        forced
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Quit))
    );
}
