use super::builders::{ORDERS, SEND_CONNECTION_STRING, send_state, state, stopped};
use super::journeys::{
    CAPTURED, Capturing, connect_and_capture, namespace_of, orders, send_batched,
};
use super::services::ServiceCall;
use super::{Harness, has_text};
use crate::archive::ArchiveInput;
use crate::archive::bulk::BulkExport;
use crate::archive::test_support::{cursor, messages, page};
use crate::bridge::EngineDone;
use crate::design::tokens::{WINDOW_MIN, WINDOW_SIZE};
use crate::form::{AuthChoice, CredentialChoice};
use crate::read::inspector::{InspectorInput, export_file_name};
use crate::read::rows::RowSource;
use crate::send::SendInput;
use crate::send::compose::PacingChoice;
use crate::state::{Input, ListSection};
use crate::views::{BODY_KEY, COUNT_KEY, INTERVAL_KEY, SEARCH_KEY, SEND_BUTTON_KEY, SWITCH_KEY};
use eh_core::engine::EngineCommand;
use eh_core::producer::{SendEnd, SendSummary};
use eh_persistence::export::ExportOutcome;
use rune::{FileDialogKind, Key};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;

fn quits(h: &Harness) -> usize {
    h.calls.count(|call| matches!(call, ServiceCall::Quit))
}

fn finished(sent: u64, end: SendEnd) -> SendSummary {
    SendSummary {
        sent,
        batches: sent,
        elapsed: Duration::from_millis(10),
        end,
    }
}

#[test]
fn connecting_from_the_empty_state_reaches_a_capturing_list() {
    let Capturing { request, .. } = connect_and_capture(ORDERS);

    assert_eq!(namespace_of(&request), "orders.servicebus.windows.net");
}

#[test]
fn a_quick_search_narrows_the_list_and_down_moves_to_the_next_match() {
    let Capturing { mut harness, .. } = connect_and_capture(ORDERS);
    harness.type_into(SEARCH_KEY, "refund");
    assert!(harness.answer_rebuild(), "the search asked for a rebuild");

    harness.press("p0-s41");
    harness.key(Key::Down);

    assert_eq!(RowSource::count(&harness.state().read), CAPTURED / 2);
    let selected = harness.state().read.inspector.selected.as_ref();
    assert_eq!(selected.map(|s| s.key_str.as_str()), Some("p0-s43"));
    assert!(harness.has_key("inspector"));
}

/// Journey 2 up to the open save dialog: a row near the followed tail selected, Export pressed.
fn export_dialog_open() -> Harness {
    let Capturing { mut harness, .. } = connect_and_capture(ORDERS);
    harness.press("p0-s43");
    harness.press("export-message");
    harness
}

#[test]
fn exporting_the_selected_message_opens_a_save_dialog_named_for_it() {
    let h = export_dialog_open();

    let Some((kind, options)) = h.rt.file_dialog() else {
        panic!("no dialog opened");
    };
    let selected = h.state().read.inspector.selected.as_ref().map(|s| &s.key);
    let expected = selected.map(|key| export_file_name(key, h.state().read.inspector.format));
    assert_eq!(kind, FileDialogKind::Save);
    assert_eq!(options.file_name, expected);
    assert!(
        options
            .file_name
            .as_deref()
            .is_some_and(|name| name.contains("43")),
        "{:?}",
        options.file_name
    );
}

#[test]
fn a_chosen_file_exports_the_message_once_and_the_button_confirms() {
    let mut h = export_dialog_open();
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("p0-s3.txt");

    h.answer_dialog(Some(&destination));
    let calls = h.take_calls();
    let [ServiceCall::ExportOne(request)] = calls.as_slice() else {
        panic!("expected one ExportOne, got {calls:?}");
    };
    let (owner, key) = (request.owner, request.key.clone());
    let now = h.now();
    h.deliver(Input::inspector(
        ListSection::Read,
        InspectorInput::ExportDone {
            owner,
            key,
            result: Ok(()),
            now,
        },
    ));

    assert_eq!(request.destination, destination);
    assert!(has_text(&h.rt, "Exported"));
}

#[test]
fn a_cancelled_save_dialog_exports_nothing() {
    let mut h = export_dialog_open();

    h.answer_dialog(None);

    assert!(h.take_calls().is_empty());
    assert!(has_text(&h.rt, "Export…"));
}

/// The Export section with a first page of 300 results and more to come.
fn export_with_results() -> Harness {
    let mut h = Harness::new(state(), WINDOW_SIZE);
    h.command(Key::Command('2'));
    h.press("archive-search-button");
    let calls = h.take_calls();
    let [ServiceCall::SearchArchive(request)] = calls.as_slice() else {
        panic!("expected one SearchArchive, got {calls:?}");
    };
    let generation = request.generation;
    h.deliver(Input::Archive(ArchiveInput::PageLoaded {
        generation,
        elapsed_ms: 1,
        result: Ok(page(messages(300), Some(cursor()))),
    }));
    h
}

fn wheel_to_the_end(h: &mut Harness) {
    for _ in 0..40 {
        h.rt.wheel(640.0, 500.0, -2_000.0);
        h.rt.frame(super::FRAME);
    }
    h.settle();
}

#[test]
fn scrolling_the_results_asks_for_one_page_and_none_while_it_loads() {
    let mut h = export_with_results();

    wheel_to_the_end(&mut h);
    let first = h.calls.searches();
    wheel_to_the_end(&mut h);

    assert!(h.has_key("p0-s0") || h.has_key("p0-s299"));
    assert_eq!(first, 1);
    assert_eq!(h.calls.searches(), 1);
}

#[test]
fn a_bulk_export_cancelled_from_its_bar_ends_in_done_and_returns_to_idle() {
    let mut h = export_with_results();
    h.press("bulk-export");
    let calls = h.take_calls();
    let [ServiceCall::BulkExport(request)] = calls.as_slice() else {
        panic!("expected one BulkExport, got {calls:?}");
    };
    let (export, cancel) = (request.export, Arc::clone(&request.cancel));

    h.press("bulk-cancel");
    let cancelling = has_text(&h.rt, "Canceling");
    h.deliver(Input::Archive(ArchiveInput::BulkDone {
        export,
        elapsed_ms: 1,
        result: Ok(ExportOutcome {
            exported: 12,
            cancelled: true,
            ..ExportOutcome::default()
        }),
    }));
    let done = h.has_key("bulk-done");
    h.press("bulk-done");

    assert!(cancel.load(Ordering::Acquire));
    assert!(cancelling && done);
    assert!(matches!(h.state().archive.bulk, BulkExport::Idle));
}

#[test]
fn a_batched_send_walks_connecting_sending_and_sent() {
    let (h, request) = send_batched(SEND_CONNECTION_STRING);

    assert!(request.template.is_none());
    assert!(h.state().send.run.outcome_line().is_some());
}

/// The Send section with Paced picked and a ten-event run one second apart started from the
/// Send button.
fn paced_send() -> (Harness, u64, Arc<AtomicBool>) {
    let (mut s, _) = send_state();
    s.send.composer.pacing = PacingChoice::Paced;
    let mut h = Harness::new(s, WINDOW_SIZE);
    h.type_into(INTERVAL_KEY, "1000");
    h.type_into(COUNT_KEY, "10");
    h.press(SEND_BUTTON_KEY);
    let calls = h.take_calls();
    let [ServiceCall::StartSend(request)] = calls.as_slice() else {
        panic!("expected one StartSend, got {calls:?}");
    };
    assert_eq!(request.plan.pacing().describe(), "every 1000 ms");
    (h, request.run, Arc::clone(&request.cancel))
}

#[test]
fn cancel_during_a_paced_wait_sets_the_flag_the_worker_polls() {
    let (mut h, run, cancel) = paced_send();
    let at = h.now();
    h.deliver(Input::Send(SendInput::Started {
        run,
        target: orders(),
        at,
    }));
    h.deliver(Input::Send(SendInput::Progress { run, sent: 1 }));

    h.press("send-cancel");
    let flagged = cancel.load(Ordering::Acquire);
    let label = h.label_of("send-status-sending");
    h.deliver(Input::Send(SendInput::Finished {
        run,
        summary: finished(1, SendEnd::Cancelled),
        close_error: None,
    }));

    assert!(flagged);
    assert_eq!(label.as_deref(), Some("Canceling"));
    assert!(h.has_key("send-status-done-cancelled"));
    assert!(!h.state().send.is_running());
}

#[test]
fn the_template_switch_decides_whether_a_send_carries_a_template() {
    let (s, _) = send_state();
    let mut h = Harness::new(s, WINDOW_SIZE);
    h.press(SWITCH_KEY);
    h.type_into(BODY_KEY, "{{random.integer min=4 max=4}}");
    let sample = h.rt.elements().into_iter().any(|element| {
        element.path.contains("/send-sample/") && element.text.as_deref() == Some("4")
    });
    let on = send_and_finish(&mut h);
    h.press(SWITCH_KEY);
    let off = send_and_finish(&mut h);

    assert!(sample);
    assert!(on.template.is_some());
    assert!(off.template.is_none());
}

/// Presses Send, ends the run it started, and returns its request.
fn send_and_finish(h: &mut Harness) -> crate::send::SendRequest {
    h.press(SEND_BUTTON_KEY);
    let mut calls = h.take_calls();
    let Some(ServiceCall::StartSend(request)) = calls.pop().filter(|_| calls.is_empty()) else {
        panic!("expected one StartSend");
    };
    h.deliver(Input::Send(SendInput::Finished {
        run: request.run,
        summary: finished(0, SendEnd::Completed),
        close_error: None,
    }));
    request
}

#[test]
fn an_invalid_template_blocks_the_send_and_moves_focus_into_the_body() {
    let (s, _) = send_state();
    let mut h = Harness::new(s, WINDOW_SIZE);
    h.press(SWITCH_KEY);
    h.type_into(BODY_KEY, "{{person.fullname}}");
    let named_before_press = has_text(&h.rt, "Line 1, column 1");
    h.press(SEND_BUTTON_KEY);

    assert!(named_before_press);
    assert!(h.take_calls().is_empty());
    assert!(h.state().send.composer.body.focused);
    assert!(has_text(&h.rt, "Fix 1 field to send"));
}

#[test]
fn field_issues_block_the_send_and_the_interval_issue_is_reachable_at_the_smallest_size() {
    let (mut s, _) = send_state();
    s.send.composer.pacing = PacingChoice::Paced;
    let mut h = Harness::new(s, WINDOW_MIN);
    h.type_into(COUNT_KEY, "0");
    h.type_into(INTERVAL_KEY, "0");

    h.press(SEND_BUTTON_KEY);
    let calls = h.take_calls();
    let count_focused = h.state().send.composer.count.focused;
    for _ in 0..20 {
        h.rt.wheel(120.0, 400.0, -2_000.0);
        h.rt.frame(super::FRAME);
    }
    h.settle();

    assert!(calls.is_empty());
    assert!(count_focused);
    assert!(has_text(&h.rt, "Fix 2 fields to send"));
    assert!(super::text_visible(
        &h.rt,
        "Enter an interval from 1 to 3,600,000 ms (one hour)"
    ));
}

/// What journey 5 started before the window closed.
struct Busy {
    harness: Harness,
    session: Option<(u64, mpsc::Receiver<EngineCommand>)>,
    bulk: Option<(u64, Arc<AtomicBool>)>,
    send: Option<(u64, Arc<AtomicBool>)>,
}

/// A credential-auth form, so a session start clears no typed secret and Send keeps its hub;
/// then the jobs asked for, each started through its own inputs.
fn busy(session: bool, bulk: bool, send: bool) -> Busy {
    let mut s = state();
    s.form.set_auth(AuthChoice::Credential);
    s.form.credential = CredentialChoice::DeveloperTools;
    s.form
        .namespace
        .replace_text("orders.servicebus.windows.net");
    s.form.event_hub.replace_text("orders");
    s.send.refresh_destination(&s.form);
    let mut h = Harness::new(s, WINDOW_SIZE);
    let (mut session_job, mut bulk_job, mut send_job) = (None, None, None);
    if session {
        session_job = Some(h.start_session(1));
    }
    if bulk {
        h.deliver(Input::Archive(ArchiveInput::SearchPressed));
        let Some(ServiceCall::SearchArchive(request)) = h.take_calls().pop() else {
            panic!("Search asked for no page");
        };
        h.deliver(Input::Archive(ArchiveInput::PageLoaded {
            generation: request.generation,
            elapsed_ms: 1,
            result: Ok(page(messages(3), None)),
        }));
        h.deliver(Input::Archive(ArchiveInput::BulkExportPressed));
        let Some(ServiceCall::BulkExport(request)) = h.take_calls().pop() else {
            panic!("the bulk export did not start");
        };
        bulk_job = Some((request.export, request.cancel));
    }
    if send {
        h.deliver(Input::Send(SendInput::SendPressed));
        let Some(ServiceCall::StartSend(request)) = h.take_calls().pop() else {
            panic!("Send did not start a run");
        };
        send_job = Some((request.run, request.cancel));
    }
    Busy {
        harness: h,
        session: session_job,
        bulk: bulk_job,
        send: send_job,
    }
}

#[rstest::rstest]
#[case::session(true, false, false)]
#[case::bulk_export(false, true, false)]
#[case::send(false, false, true)]
#[case::all_three(true, true, true)]
fn closing_the_window_stops_every_job_and_quits_once_after_the_last_ends(
    #[case] session: bool,
    #[case] bulk: bool,
    #[case] send: bool,
) {
    let Busy {
        harness: mut h,
        session,
        bulk,
        send,
    } = busy(session, bulk, send);
    let now = h.now();

    h.deliver(Input::WindowClosed(now));
    let flags =
        [&bulk, &send].map(|job| job.as_ref().map(|(_, flag)| flag.load(Ordering::Acquire)));
    let mut stops = Vec::new();
    let mut quits_before_last = Vec::new();
    if let Some((id, mut commands)) = session {
        stops.push(commands.try_recv());
        quits_before_last.push(quits(&h));
        h.deliver(Input::EngineDone(EngineDone {
            session: id,
            engine: Ok(stopped()),
            finish: Ok(ExportOutcome::default()),
        }));
    }
    if let Some((export, _)) = bulk {
        quits_before_last.push(quits(&h));
        h.deliver(Input::Archive(ArchiveInput::BulkDone {
            export,
            elapsed_ms: 1,
            result: Ok(ExportOutcome {
                cancelled: true,
                ..ExportOutcome::default()
            }),
        }));
    }
    if let Some((run, _)) = send {
        quits_before_last.push(quits(&h));
        h.deliver(Input::Send(SendInput::Finished {
            run,
            summary: finished(0, SendEnd::Cancelled),
            close_error: None,
        }));
    }

    assert!(
        stops
            .iter()
            .all(|stop| matches!(stop, Ok(EngineCommand::Stop))),
        "{stops:?}"
    );
    assert!(flags.iter().flatten().all(|set| *set));
    assert!(quits_before_last.iter().all(|count| *count == 0));
    assert_eq!(quits(&h), 1);
}

#[test]
fn a_send_that_fails_with_the_window_closed_reopens_it_with_a_toast() {
    let Busy {
        harness: mut h,
        send,
        ..
    } = busy(false, false, true);
    let Some((run, _)) = send else {
        panic!("no send started");
    };
    let now = h.now();
    h.deliver(Input::WindowClosed(now));

    h.deliver(Input::Send(SendInput::Finished {
        run,
        summary: finished(0, SendEnd::Failed("the hub refused the batch".to_owned())),
        close_error: None,
    }));

    assert_eq!(
        h.calls
            .count(|call| matches!(call, ServiceCall::ReopenWindow)),
        1
    );
    assert_eq!(quits(&h), 0);
    assert!(
        h.state()
            .toasts()
            .entries()
            .iter()
            .any(|toast| toast.options.text.contains("the hub refused the batch"))
    );
}

#[test]
fn return_in_a_sheet_field_presses_connect() {
    let mut h = Harness::new(state(), WINDOW_SIZE);
    h.press("empty-connect");
    h.type_into("connection-string", ORDERS);

    h.key(Key::Enter);

    let calls = h.take_calls();
    assert!(
        matches!(calls.as_slice(), [ServiceCall::PrepareSession { .. }]),
        "{calls:?}"
    );
    assert!(h.state().sheet.is_none());
}

#[test]
fn return_in_the_send_target_sheet_presses_done() {
    let (mut s, _) = send_state();
    s.sheet = Some(crate::state::SheetPurpose::SendTarget);
    let mut h = Harness::new(s, WINDOW_SIZE);
    h.press("connection-string");

    h.key(Key::Enter);

    assert!(h.state().sheet.is_none());
    assert!(h.take_calls().is_empty());
}

#[test]
fn a_failed_connect_offers_connect_again_with_the_form_intact() {
    let mut s = state();
    s.form.connection_string.edit_mut().replace_text(ORDERS);
    let mut h = Harness::new(s, WINDOW_SIZE);
    h.deliver(Input::ConnectPressed);
    let Some(ServiceCall::PrepareSession { attempt, .. }) = h.take_calls().pop() else {
        panic!("Connect did not prepare a session");
    };
    h.deliver(Input::SessionReady {
        attempt,
        result: Err("the archive is in use by another eh".to_owned()),
    });
    let failed = has_text(&h.rt, "Couldn't connect");

    h.press("empty-connect");

    assert!(failed);
    assert!(h.state().sheet.is_some());
    assert_eq!(h.state().form.connection_string.edit().text, ORDERS);
}
