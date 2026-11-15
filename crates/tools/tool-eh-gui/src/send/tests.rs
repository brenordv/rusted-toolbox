use super::compose::{BODY_CAP, PacingChoice, RoutingChoice};
use super::*;
use chrono::TimeDelta;
use rstest::rstest;
use shared_eventhub_core::producer::{Pacing, Routing};
use std::sync::atomic::Ordering;
use std::time::Duration;

const CONNECTION_STRING: &str = "Endpoint=sb://orders.servicebus.windows.net/;\
    SharedAccessKeyName=send;SharedAccessKey=c2VjcmV0;EntityPath=orders";

fn form() -> ConnectForm {
    let mut form = ConnectForm::default();
    form.connection_string
        .edit_mut()
        .replace_text(CONNECTION_STRING);
    form
}

fn ready() -> SendState {
    let mut send = SendState::default();
    send.refresh_destination(&form());
    send
}

fn cx(form: &ConnectForm) -> PressContext<'_> {
    PressContext {
        sheet_open: false,
        window_closed: false,
        form,
    }
}

/// Presses Send on a ready state; returns the run id.
fn pressed(send: &mut SendState) -> u64 {
    let form = form();
    let out = send.apply(SendInput::SendPressed, cx(&form));
    match out.effects.as_slice() {
        [SendEffect::StartSend(request)] => request.run,
        other => panic!("expected StartSend, got {other:?}"),
    }
}

fn orders() -> Names {
    Names {
        namespace: "orders.servicebus.windows.net".to_owned(),
        event_hub: "orders".to_owned(),
    }
}

fn sending(send: &mut SendState, at: DateTime<Utc>) -> u64 {
    let run = pressed(send);
    let form = form();
    send.apply(
        SendInput::Started {
            run,
            target: orders(),
            at,
        },
        cx(&form),
    );
    run
}

fn summary(sent: u64, end: SendEnd) -> SendSummary {
    SendSummary {
        sent,
        batches: sent,
        elapsed: Duration::from_secs(1),
        end,
    }
}

fn apply(send: &mut SendState, input: SendInput) -> SendOutcome {
    let form = form();
    send.apply(input, cx(&form))
}

#[test]
fn a_typed_connection_string_names_its_hub() {
    let send = ready();

    assert_eq!(send.destination, Destination::Named(orders()));
}

#[test]
fn a_form_with_issues_has_no_destination() {
    let mut send = SendState::default();

    send.refresh_destination(&ConnectForm::default());

    assert_eq!(send.destination, Destination::Missing);
}

#[test]
fn a_typed_string_core_refuses_is_invalid() {
    let mut form = form();
    form.event_hub.replace_text("payments");
    let mut send = SendState::default();

    send.refresh_destination(&form);

    assert_eq!(send.destination, Destination::Invalid);
}

#[test]
fn a_saved_connection_string_is_known_only_at_run_time() {
    let mut form = ConnectForm::default();
    form.connection_string = crate::form::SecretSource::stored();
    let mut send = SendState::default();

    send.refresh_destination(&form);

    assert_eq!(send.destination, Destination::Stored);
}

#[test]
fn a_press_starts_a_run_for_the_composed_plan() {
    let mut send = ready();
    send.composer.count.replace_text("1000");
    let form = form();

    let out = send.apply(SendInput::SendPressed, cx(&form));

    let [SendEffect::StartSend(request)] = out.effects.as_slice() else {
        panic!("expected StartSend, got {:?}", out.effects);
    };
    assert_eq!(request.plan.count(), 1000);
    assert!(matches!(
        &send.run,
        SendRun::Preparing { requested: 1000, target: Some(target), .. } if *target == orders()
    ));
}

#[rstest]
#[case::sheet_open(true, false)]
#[case::window_closed(false, true)]
fn a_press_is_ignored_with_a_sheet_open_or_the_window_closed(
    #[case] sheet_open: bool,
    #[case] window_closed: bool,
) {
    let mut send = ready();
    let form = form();

    let out = send.apply(
        SendInput::SendPressed,
        PressContext {
            sheet_open,
            window_closed,
            form: &form,
        },
    );

    assert!(out.effects.is_empty() && !out.changed);
    assert!(matches!(send.run, SendRun::Idle));
}

#[test]
fn a_press_while_a_run_is_running_is_ignored() {
    let mut send = ready();
    let run = pressed(&mut send);

    let out = apply(&mut send, SendInput::SendPressed);

    assert!(out.effects.is_empty());
    assert_eq!(send.run.running_id(), Some(run));
}

#[test]
fn a_press_with_issues_shows_them_and_asks_for_the_first_field() {
    let mut send = ready();
    send.composer.count.replace_text("0");
    send.composer.pacing = PacingChoice::Paced;
    send.composer.interval.replace_text("abc");

    let out = apply(&mut send, SendInput::SendPressed);

    assert!(out.effects.is_empty() && out.changed);
    assert!(send.show_issues);
    assert_eq!(send.shown_issues().len(), 2);
    assert_eq!(send.take_focus(), Some(SendField::Count));
}

#[test]
fn issues_stay_hidden_until_the_first_press() {
    let mut send = ready();
    send.composer.count.replace_text("0");

    assert!(send.shown_issues().is_empty());
}

#[rstest]
#[case::batched(PacingChoice::Batched, "abc", None)]
#[case::paced(PacingChoice::Paced, "250", Some(250))]
#[case::paced_shortest(PacingChoice::Paced, "1", Some(1))]
#[case::paced_longest(PacingChoice::Paced, "3600000", Some(3_600_000))]
fn the_composer_maps_pacing_to_the_plan(
    #[case] choice: PacingChoice,
    #[case] interval: &str,
    #[case] expected_ms: Option<u64>,
) {
    let mut send = ready();
    send.composer.pacing = choice;
    send.composer.interval.replace_text(interval);

    let (plan, _) = send.composer.compose(true).unwrap();

    assert_eq!(
        plan.pacing(),
        match expected_ms {
            Some(ms) => Pacing::interval(Duration::from_millis(ms)).unwrap(),
            None => Pacing::batched(),
        }
    );
}

#[test]
fn the_composer_maps_routing_content_type_and_body() {
    let mut send = ready();
    send.composer.routing = RoutingChoice::PartitionKey;
    send.composer.partition_key.replace_text("customer-17");
    send.composer
        .content_type
        .replace_text(" application/json ");
    send.composer.body.replace_text("{\"orderId\": 1}");
    send.composer.count.replace_text(" 5 ");

    let (plan, event) = send.composer.compose(true).unwrap();

    assert_eq!(
        plan.routing(),
        &Routing::partition_key("customer-17").unwrap()
    );
    assert_eq!(plan.count(), 5);
    assert_eq!(event.content_type.as_deref(), Some("application/json"));
    assert_eq!(event.body, b"{\"orderId\": 1}");
}

#[test]
fn an_empty_content_type_and_body_are_allowed() {
    let send = ready();

    let (_, event) = send.composer.compose(true).unwrap();

    assert_eq!(event.content_type, None);
    assert!(event.body.is_empty());
}

#[rstest]
#[case::zero("0")]
#[case::over_the_cap("1000001")]
#[case::letters("abc")]
#[case::signed("+5")]
#[case::empty("")]
#[case::past_u64("1234567890123456789012345")]
fn a_bad_count_is_its_issue(#[case] count: &str) {
    let mut send = ready();
    send.composer.count.replace_text(count);

    let issues = send.composer.issues(true);

    assert_eq!(
        issues.count.as_deref(),
        Some("Enter a number from 1 to 1,000,000")
    );
    assert_eq!(issues.len(), 1);
}

#[rstest]
#[case::zero("0")]
#[case::over_an_hour("3600001")]
#[case::letters("abc")]
#[case::signed("+5")]
#[case::empty("")]
fn a_bad_interval_is_its_issue_while_paced(#[case] interval: &str) {
    let mut send = ready();
    send.composer.pacing = PacingChoice::Paced;
    send.composer.interval.replace_text(interval);

    let issues = send.composer.issues(true);

    assert_eq!(
        issues.interval.as_deref(),
        Some("Enter an interval from 1 to 3,600,000 ms (one hour)")
    );
    assert_eq!(issues.len(), 1);
}

#[test]
fn an_empty_partition_key_is_its_issue() {
    let mut send = ready();
    send.composer.routing = RoutingChoice::PartitionKey;
    send.composer.partition_key.replace_text("  ");

    let issues = send.composer.issues(true);

    assert_eq!(
        issues.partition_key.as_deref(),
        Some("Enter a partition key, or pick Automatic")
    );
}

#[test]
fn a_body_over_the_cap_is_its_issue() {
    let mut send = ready();
    send.composer.body.replace_text("x".repeat(BODY_CAP + 1));

    let issues = send.composer.issues(true);

    assert!(issues.body.is_some());
    assert_eq!(issues.first(), Some(SendField::Body));
}

#[test]
fn a_body_at_the_cap_has_no_issue() {
    let mut send = ready();
    send.composer.body.replace_text("x".repeat(BODY_CAP));

    assert!(send.composer.issues(true).is_empty());
}

#[test]
fn no_target_is_the_first_issue() {
    let mut send = SendState::default();
    send.composer.count.replace_text("0");

    let issues = send.composer.issues(send.destination.is_ready());

    assert_eq!(
        issues.target.as_deref(),
        Some("Set up the connection first")
    );
    assert_eq!(issues.first(), Some(SendField::Target));
}

#[rstest]
#[case::batched(PacingChoice::Batched, "1000", "1,000 events to orders")]
#[case::paced(
    PacingChoice::Paced,
    "1000",
    "1,000 events to orders, one every 1,000 ms"
)]
#[case::paced_invalid(PacingChoice::Paced, "0", "1,000 events to orders")]
fn the_summary_names_only_valid_parts(
    #[case] choice: PacingChoice,
    #[case] interval: &str,
    #[case] expected: &str,
) {
    let mut send = ready();
    send.composer.count.replace_text("1000");
    send.composer.pacing = choice;
    send.composer.interval.replace_text(interval);

    assert_eq!(send.composer.summary(Some("orders")), expected);
}

#[test]
fn the_summary_names_a_partition_key() {
    let mut send = ready();
    send.composer.routing = RoutingChoice::PartitionKey;
    send.composer.partition_key.replace_text("customer-17");

    assert_eq!(
        send.composer.summary(Some("orders")),
        "1 event to orders, partition key customer-17"
    );
}

#[test]
fn started_moves_the_current_run_to_sending() {
    let mut send = ready();
    let at = Utc::now();

    let run = sending(&mut send, at);

    assert!(matches!(
        &send.run,
        SendRun::Sending { run: current, sent: 0, started, .. } if *current == run && *started == at
    ));
}

#[test]
fn progress_is_kept_and_shown_on_the_next_tick() {
    let mut send = ready();
    let at = Utc::now();
    let run = sending(&mut send, at);

    let out = apply(&mut send, SendInput::Progress { run, sent: 340 });
    let ticked = send.tick(at);

    assert!(!out.changed);
    assert!(ticked);
    assert!(matches!(
        send.run,
        SendRun::Sending {
            sent: 340,
            shown: 340,
            ..
        }
    ));
}

#[test]
fn ticks_move_the_elapsed_time_once_a_second_and_never_back() {
    let mut send = ready();
    let at = Utc::now();
    sending(&mut send, at);

    let first = send.tick(at + TimeDelta::milliseconds(1_500));
    let same = send.tick(at + TimeDelta::milliseconds(1_900));
    let back = send.tick(at - TimeDelta::seconds(5));

    assert!(first && !same && !back);
    assert!(matches!(
        send.run,
        SendRun::Sending {
            elapsed_secs: 1,
            ..
        }
    ));
}

#[test]
fn cancel_sets_the_flag_once() {
    let mut send = ready();
    let run = sending(&mut send, Utc::now());
    let cancel = match &send.run {
        SendRun::Sending { cancel, .. } => Arc::clone(cancel),
        other => panic!("expected Sending, got {other:?}"),
    };

    let first = apply(&mut send, SendInput::CancelPressed);
    let second = apply(&mut send, SendInput::CancelPressed);

    assert!(first.changed && !second.changed);
    assert!(cancel.load(Ordering::Acquire));
    assert!(send.run.cancel_requested());
    assert_eq!(send.run.running_id(), Some(run));
}

#[test]
fn a_window_close_cancels_a_preparing_run() {
    let mut send = ready();
    pressed(&mut send);

    let changed = send.request_cancel(CancelReason::WindowClosed);

    assert!(changed && send.run.cancel_requested());
}

#[test]
fn a_cancelled_end_keeps_the_partial_count() {
    let mut send = ready();
    send.composer.count.replace_text("1000");
    let run = sending(&mut send, Utc::now());
    apply(&mut send, SendInput::CancelPressed);

    let out = apply(
        &mut send,
        SendInput::Finished {
            run,
            summary: summary(340, SendEnd::Cancelled),
            close_error: None,
        },
    );

    assert_eq!(
        out.ended,
        Some((false, "Canceled after 340 of 1,000".to_owned()))
    );
    assert!(matches!(send.run, SendRun::Done { .. }));
}

#[rstest]
#[case::completed(summary(1, SendEnd::Completed), None, false, "Sent 1 event")]
#[case::failed_before_sending(
    summary(0, SendEnd::Failed("unauthorized".to_owned())),
    None,
    true,
    "Couldn't send to orders: unauthorized"
)]
#[case::close_failed(
    summary(1, SendEnd::Completed),
    Some("link lost"),
    true,
    "Sent 1 event"
)]
fn the_end_of_a_run_says_how_it_ended(
    #[case] summary: SendSummary,
    #[case] close_error: Option<&str>,
    #[case] failed: bool,
    #[case] line: &str,
) {
    let mut send = ready();
    let run = sending(&mut send, Utc::now());

    let out = apply(
        &mut send,
        SendInput::Finished {
            run,
            summary,
            close_error: close_error.map(str::to_owned),
        },
    );

    assert_eq!(out.ended, Some((failed, line.to_owned())));
}

#[test]
fn a_failure_before_any_names_were_known_says_so_without_a_hub() {
    let mut form = ConnectForm::default();
    form.connection_string = crate::form::SecretSource::stored();
    let mut send = SendState::default();
    send.refresh_destination(&form);
    let out = send.apply(SendInput::SendPressed, cx(&form));
    let [SendEffect::StartSend(request)] = out.effects.as_slice() else {
        panic!("expected StartSend, got {:?}", out.effects);
    };
    let run = request.run;

    send.apply(
        SendInput::Finished {
            run,
            summary: summary(0, SendEnd::Failed("no connection string stored".to_owned())),
            close_error: None,
        },
        cx(&form),
    );

    assert_eq!(
        send.run.outcome_line().as_deref(),
        Some("Couldn't send: no connection string stored")
    );
}

#[test]
fn a_failure_midway_names_the_count() {
    let mut send = ready();
    send.composer.count.replace_text("1000");
    let run = sending(&mut send, Utc::now());

    apply(
        &mut send,
        SendInput::Finished {
            run,
            summary: summary(340, SendEnd::Failed("throttled".to_owned())),
            close_error: None,
        },
    );

    assert_eq!(
        send.run.outcome_line().as_deref(),
        Some("Stopped after 340 of 1,000: throttled")
    );
}

#[test]
fn inputs_for_another_run_change_nothing() {
    let mut send = ready();
    let run = sending(&mut send, Utc::now());

    let started = apply(
        &mut send,
        SendInput::Started {
            run,
            target: orders(),
            at: Utc::now(),
        },
    );
    let progress = apply(
        &mut send,
        SendInput::Progress {
            run: run + 1,
            sent: 9,
        },
    );
    let finished = apply(
        &mut send,
        SendInput::Finished {
            run: run + 1,
            summary: summary(1, SendEnd::Completed),
            close_error: None,
        },
    );

    assert!(!started.changed && !progress.changed && !finished.changed);
    assert!(matches!(send.run, SendRun::Sending { sent: 0, .. }));
}

#[test]
fn progress_while_preparing_changes_nothing() {
    let mut send = ready();
    let run = pressed(&mut send);

    apply(&mut send, SendInput::Progress { run, sent: 5 });

    assert!(matches!(send.run, SendRun::Preparing { .. }));
}

#[test]
fn a_second_finish_after_done_is_dropped() {
    let mut send = ready();
    let run = sending(&mut send, Utc::now());
    apply(
        &mut send,
        SendInput::Finished {
            run,
            summary: summary(0, SendEnd::Failed("did not stop".to_owned())),
            close_error: None,
        },
    );

    let late = apply(
        &mut send,
        SendInput::Finished {
            run,
            summary: summary(1, SendEnd::Completed),
            close_error: None,
        },
    );

    assert!(late.ended.is_none());
    assert!(matches!(
        &send.run,
        SendRun::Done { summary, .. } if summary.sent == 0
    ));
}

#[test]
fn a_run_keeps_the_pacing_of_its_press() {
    let mut send = ready();
    send.composer.pacing = PacingChoice::Paced;
    sending(&mut send, Utc::now());

    send.composer.pacing = PacingChoice::Batched;

    let (_, after) = send.run.running_words().unwrap();
    assert!(after.ends_with(", one every 1,000 ms"), "{after}");
}
