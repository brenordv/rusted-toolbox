use super::Harness;
use super::builders::{ORDERS, send_state, state};
use super::services::ServiceCall;
use super::sweep_tests::{failed_with, orders_form, send_done, send_sending_run, sheet};
use super::worst_cases::{long_archive_error, long_bulk_failure, long_text};
use crate::bridge::UiDelivery;
use crate::design::tokens::WINDOW_SIZE;
use crate::form::AuthChoice;
use crate::read::inspector::FEEDBACK_FOR;
use crate::send::SendInput;
use crate::state::{ConnectionState, Input, Section, SheetPurpose, error_preview};
use crate::views::details::{COPY_KEY, DONE_KEY, FAILURE_KEY};
use crate::views::shell::open_log_line;
use chrono::Utc;
use common_cli::app_logger::FileLogging;
use rune::accesskit::Action;
use rune::{Key, KeyEvent, Mods};
use shared_eventhub_core::engine::EngineEvent;
use shared_eventhub_core::producer::{SendEnd, SendSummary};
use std::time::Duration;

const DIALOG: &str = "details-dialog";

fn long_failure() -> Harness {
    failed_with(WINDOW_SIZE, &long_text())
}

/// A connect failure with `text` that lands while the Export section shows, so it toasts.
fn toasted_failure_with(text: &str) -> Harness {
    let mut h = Harness::new(orders_form(), WINDOW_SIZE);
    h.deliver(Input::SectionPicked(Section::Export));
    h.deliver(Input::ConnectPressed);
    let attempt = match &h.state().connection {
        ConnectionState::Connecting { attempt, .. } => *attempt,
        other => panic!("not preparing: {other:?}"),
    };
    h.deliver(Input::SessionReady {
        attempt,
        result: Err(text.to_owned()),
    });
    h
}

fn toasted_failure() -> Harness {
    toasted_failure_with(&long_text())
}

/// The labels of every element inside the element keyed `within`.
fn labels_within(h: &Harness, within: &str) -> Vec<String> {
    let inside = h.path_of(within);
    h.rt.elements()
        .into_iter()
        .filter(|element| element.path.starts_with(&format!("{inside}/")))
        .filter_map(|element| element.label)
        .collect()
}

fn toast_key(h: &Harness) -> String {
    format!("toast-{}", h.state().toasts[0].id)
}

fn details_text(h: &Harness) -> String {
    h.state()
        .details
        .as_ref()
        .map(|details| details.text.clone())
        .unwrap_or_default()
}

fn details_title(h: &Harness) -> String {
    h.state()
        .details
        .as_ref()
        .map(|details| details.title.clone())
        .unwrap_or_default()
}

#[test]
fn a_long_failure_toast_shows_a_preview_and_opens_and_copies_the_whole_text() {
    let mut h = toasted_failure();
    let failure = long_text();
    let toasted = format!("Could not connect: {failure}");
    let toast = toast_key(&h);

    let shown_whole_before = h.has_text(&failure);
    let preview_shown = h.has_text(&error_preview(&toasted).shown);
    h.press_named(&toast, "View details");

    assert!(!shown_whole_before);
    assert!(preview_shown);
    assert!(h.has_key(DIALOG));
    assert_eq!(details_title(&h), "Error details");
    assert_eq!(details_text(&h), toasted);
    assert!(h.has_text(&toasted));
    h.press(COPY_KEY);
    assert_eq!(h.clipboard_text().as_deref(), Some(toasted.as_str()));
    assert_eq!(h.label_of(COPY_KEY).as_deref(), Some("Copied"));
    h.deliver(Input::Tick(Utc::now() + FEEDBACK_FOR));
    assert_eq!(h.label_of(COPY_KEY).as_deref(), Some("Copy error"));
    h.key(Key::Escape);
    assert!(!h.has_key(DIALOG));
    assert_eq!(
        h.focused_label_within(&toast).as_deref(),
        Some("View details")
    );
}

#[test]
fn the_read_empty_state_shows_the_preview_of_a_long_failure_and_no_toast_repeats_it() {
    let h = long_failure();
    let failure = long_text();

    assert!(h.has_text(&error_preview(&failure).shown));
    assert!(!h.has_text(&failure));
    assert!(h.has_key(FAILURE_KEY));
    assert!(h.state().toasts.is_empty());
}

#[test]
fn the_empty_states_button_opens_the_failure_and_the_close_button_returns_focus_to_it() {
    let mut h = long_failure();

    h.press(FAILURE_KEY);
    let title = details_title(&h);
    let text = details_text(&h);
    h.press_named(DIALOG, "Close");

    assert_eq!(title, "Couldn't connect");
    assert_eq!(text, long_text());
    assert!(!h.has_key(DIALOG));
    assert_eq!(super::focused_key(&h.rt).as_deref(), Some(FAILURE_KEY));
}

#[rstest::rstest]
#[case::done(Some(DONE_KEY), None)]
#[case::return_key(None, Some(Key::Enter))]
fn done_and_return_close_the_modal(#[case] press: Option<&str>, #[case] key: Option<Key>) {
    let mut h = long_failure();
    h.press(FAILURE_KEY);

    match (press, key) {
        (Some(target), _) => h.press(target),
        (None, Some(key)) => h.key(key),
        (None, None) => panic!("a case presses something"),
    }

    assert!(!h.has_key(DIALOG));
    assert!(h.state().details.is_none());
}

#[test]
fn return_with_copy_error_focused_closes_and_space_copies() {
    let mut h = long_failure();
    h.press(FAILURE_KEY);
    assert!(h.rt.accessibility_action(COPY_KEY, Action::Focus, None));
    h.settle();

    h.key(Key::Text(" ".into()));
    let copied = h.clipboard_text();
    h.key(Key::Enter);

    assert_eq!(copied.as_deref(), Some(long_text().as_str()));
    assert!(h.state().details.is_none());
}

#[test]
fn a_click_outside_the_panel_closes_the_modal() {
    let mut h = long_failure();
    h.press(FAILURE_KEY);

    h.press_at(4.0, WINDOW_SIZE.1 - 4.0);

    assert!(h.state().details.is_none());
}

#[test]
fn a_short_failure_shows_whole_in_the_empty_state_and_the_pill_with_no_details_button() {
    let mut h = failed_with(WINDOW_SIZE, SHORT);

    let in_empty_state = h.has_text(SHORT) && !h.has_key(FAILURE_KEY);
    h.press("pill");

    assert!(in_empty_state);
    assert!(h.state().pill_open);
    assert!(!h.has_key("pill-error-details"));
}

#[test]
fn a_short_failure_toast_shows_whole_with_no_details_button() {
    let h = toasted_failure_with(SHORT);
    let toast = toast_key(&h);

    let labels = labels_within(&h, &toast);

    assert!(h.has_text(&format!("Could not connect: {SHORT}")));
    assert!(
        !labels.iter().any(|label| label == "View details"),
        "{labels:?}"
    );
}

const SHORT: &str = "the archive is in use by another eh";

#[test]
fn the_pill_popovers_error_button_replaces_the_popover_and_focus_returns_to_the_pill() {
    let mut h = long_failure();
    h.press("pill");

    h.press("pill-error-details");
    let popover_closed = !h.state().pill_open;
    let text = details_text(&h);
    h.key(Key::Escape);

    assert!(popover_closed);
    assert_eq!(text, long_text());
    assert_eq!(super::focused_key(&h.rt).as_deref(), Some("pill"));
}

/// A session over four partitions whose log holds two long errors, older first.
fn logged_errors() -> (Harness, u64, Vec<u64>) {
    let mut h = Harness::new(orders_form(), WINDOW_SIZE);
    let (session, _) = h.start_session(4);
    let errors = ["older", "newer"]
        .into_iter()
        .enumerate()
        .map(|(partition, which)| EngineEvent::Error {
            partition_id: Some(partition.to_string()),
            error: format!("{which}: {}", long_text()),
            fatal: false,
        })
        .collect();
    let at = h.now();
    h.deliver(Input::Delivery(UiDelivery {
        at,
        session,
        events: errors,
        stats: None,
    }));
    let ids = h
        .state()
        .read
        .session_log
        .iter()
        .map(|line| line.id)
        .collect();
    (h, session, ids)
}

#[test]
fn a_log_lines_button_opens_its_own_line_after_a_newer_one_arrives() {
    let (mut h, session, ids) = logged_errors();
    h.press("pill");
    let key = format!("log-details-{}", ids[0]);
    let name = h.label_of(&key);
    let at = h.now();
    h.deliver(Input::Delivery(UiDelivery {
        at,
        session,
        events: vec![EngineEvent::Error {
            partition_id: Some("3".to_owned()),
            error: format!("newest: {}", long_text()),
            fatal: false,
        }],
        stats: None,
    }));

    h.press(&key);
    let text = details_text(&h);
    let title = details_title(&h);
    h.key(Key::Escape);

    assert!(text.starts_with("older: "), "{text}");
    assert!(title.ends_with("Error, partition 0"), "{title}");
    assert_eq!(name, Some(format!("View details: {title}")));
    assert_eq!(super::focused_key(&h.rt).as_deref(), Some("pill"));
}

#[test]
fn a_log_line_that_left_the_log_opens_nothing_and_moves_no_focus() {
    let (mut h, _, ids) = logged_errors();
    h.press("pill");
    let older = format!("log-details-{}", ids[0]);
    assert!(h.rt.accessibility_action(&older, Action::Focus, None));
    h.settle();
    let focus_before = super::focused_key(&h.rt);

    h.rt.update_if(|s, tx| {
        open_log_line(s, tx, u64::MAX);
        true
    });
    h.settle();

    assert_eq!(focus_before.as_deref(), Some(older.as_str()));
    assert!(h.state().details.is_none());
    assert!(h.state().pill_open);
    assert_eq!(super::focused_key(&h.rt), focus_before);
}

fn finished_with_close_error(close_error: String) -> Harness {
    let (mut h, run) = send_sending_run(WINDOW_SIZE);
    h.deliver(Input::Send(SendInput::Finished {
        run,
        summary: SendSummary {
            sent: 1,
            batches: 1,
            elapsed: Duration::from_millis(5),
            end: SendEnd::Completed,
        },
        close_error: Some(close_error),
    }));
    h
}

#[test]
fn a_long_send_failure_shows_its_preview_and_opens_the_whole_line() {
    let mut h = send_done(WINDOW_SIZE, SendEnd::Failed(long_text()));
    let line = h.state().send.run.outcome_line().unwrap();
    let status = h.label_of("send-status-done-failed");

    h.press("send-details");

    assert_eq!(status, Some(error_preview(&line).shown));
    assert_eq!(details_title(&h), "The send failed");
    assert_eq!(details_text(&h), line);
}

#[test]
fn a_long_close_error_after_a_completed_send_opens_with_its_prefix() {
    let mut h = finished_with_close_error(long_text());

    h.press("send-details");

    assert_eq!(details_title(&h), "Closing the connection failed");
    assert_eq!(
        details_text(&h),
        format!(
            "Sent 1 event\n\nClosing the connection failed: {}",
            long_text()
        )
    );
}

#[test]
fn a_long_bulk_failure_opens_its_first_error() {
    let mut h = long_bulk_failure(WINDOW_SIZE);
    h.rt.state.file_logging = FileLogging::Writing("eh-gui.log".into());
    h.rt.invalidate();
    h.settle();

    let points_at_log = h.has_text("Every failure is in the log file.");
    h.press("bulk-details");

    assert!(points_at_log);
    assert_eq!(details_title(&h), "The export failed");
    assert_eq!(details_text(&h), long_text());
}

#[test]
fn a_long_archive_error_opens_from_the_export_empty_state() {
    let mut h = long_archive_error(WINDOW_SIZE);

    h.press("archive-error-details");

    assert_eq!(details_title(&h), "Couldn't search the archive");
    assert_eq!(details_text(&h), long_text());
}

#[test]
fn a_long_config_caption_opens_over_the_sheet_and_focus_returns_to_its_button() {
    let mut s = state();
    s.config_caption = Some(format!("config.json was not loaded: {}", long_text()));
    s.sheet = Some(SheetPurpose::Connect);
    let mut h = Harness::new(s, WINDOW_SIZE);

    h.press("config-details");
    let opened = h.has_key(DIALOG) && h.has_key("connect-dialog");
    h.key(Key::Escape);

    assert!(opened);
    assert!(h.state().sheet.is_some());
    assert_eq!(super::focused_key(&h.rt).as_deref(), Some("config-details"));
}

#[test]
fn the_send_shortcuts_do_nothing_while_the_modal_is_open() {
    let (s, _) = send_state();
    let mut h = Harness::new(s, WINDOW_SIZE);
    h.deliver(Input::ShowDetails {
        title: "Error details".to_owned(),
        text: long_text(),
    });

    h.command(Key::Enter);
    h.command(Key::Command('1'));

    assert_eq!(
        h.calls
            .count(|call| matches!(call, ServiceCall::StartSend(_))),
        0
    );
    assert_eq!(h.state().section, Section::Send);
}

#[test]
fn a_failed_save_opens_over_the_sheet_and_return_closes_only_the_modal() {
    let mut h = sheet(WINDOW_SIZE, AuthChoice::ConnectionString);
    h.rt.state
        .form
        .connection_string
        .edit_mut()
        .replace_text(ORDERS);
    h.deliver(Input::ConfigSaved(Err("x".repeat(400))));
    let toast = toast_key(&h);

    h.press_named(&toast, "View details");
    let over_the_sheet = h.has_key(DIALOG) && h.has_key("connect-dialog");
    h.rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    h.settle();
    let after_return = (h.state().details.is_none(), h.state().sheet.is_some());
    h.press_named(&toast, "View details");
    h.key(Key::Escape);
    let after_escape = (h.state().details.is_none(), h.state().sheet.is_some());
    let focus = h.focused_label_within(&toast);
    h.key(Key::Escape);

    assert!(over_the_sheet);
    assert_eq!(after_return, (true, true));
    assert_eq!(
        h.calls
            .count(|call| matches!(call, ServiceCall::PrepareSession { .. })),
        0
    );
    assert_eq!(after_escape, (true, true));
    assert_eq!(focus.as_deref(), Some("View details"));
    assert!(h.state().sheet.is_none());
}
