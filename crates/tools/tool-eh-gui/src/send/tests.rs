use super::compose::{BODY_CAP, PacingChoice, RoutingChoice};
use super::template::TemplateState;
use super::*;
use chrono::TimeDelta;
use eh_core::producer::{Pacing, Routing};
use rstest::rstest;
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
        paths: DataPaths::new(form.data_folder.trim()),
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
            ..cx(&form)
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

    let plan = send.composer.compose(true).unwrap().plan;

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

    let Composed { plan, event, .. } = send.composer.compose(true).unwrap();

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

    let event = send.composer.compose(true).unwrap().event;

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

fn with_template(body: &str) -> SendState {
    let mut send = ready();
    send.composer.body.replace_text(body);
    send.composer.set_template(true);
    send
}

#[test]
fn a_template_issue_shows_only_after_a_press_and_blocks_it() {
    let mut send = with_template("{\"n\": {{person.fullname}}}");
    let before = send.shown_issues();

    let form = form();
    let out = send.apply(SendInput::SendPressed, cx(&form));

    assert!(before.is_empty());
    assert!(out.effects.is_empty());
    assert_eq!(
        send.shown_issues().body.as_deref(),
        Some("Line 1, column 7: unknown type 'person.fullname'")
    );
    assert_eq!(send.take_focus(), Some(SendField::Body));
}

#[test]
fn fixing_the_template_clears_its_issue_and_lets_the_press_through() {
    let mut send = with_template("{{person.fullname}}");
    let form = form();
    send.apply(SendInput::SendPressed, cx(&form));

    send.composer.body.replace_text("{{person.full-name}}");
    send.composer.refresh_template();
    let shown = send.shown_issues();
    let out = send.apply(SendInput::SendPressed, cx(&form));

    assert!(shown.is_empty());
    assert_eq!(out.effects.len(), 1);
}

#[test]
fn turning_the_switch_off_clears_the_template_issue() {
    let mut send = with_template("{{person.fullname}}");

    send.composer.set_template(false);

    assert!(send.composer.issues(true).is_empty());
    assert!(!send.composer.template().is_on());
}

#[test]
fn a_template_body_over_the_cap_has_the_cap_issue_not_a_compile_issue() {
    let send = with_template(&format!("{{{{nope}}}}{}", "x".repeat(BODY_CAP)));

    let issues = send.composer.issues(true);

    assert_eq!(
        issues.body.as_deref(),
        Some("Bodies over 1 MB don't fit this editor; use eh send --body-file")
    );
    assert_eq!(issues.len(), 1);
}

#[rstest]
#[case::on(true, Some(2))]
#[case::off(false, None)]
fn the_request_carries_the_template_only_while_the_switch_is_on(
    #[case] on: bool,
    #[case] placeholders: Option<usize>,
) {
    let mut send = ready();
    send.composer
        .body
        .replace_text("{{person.email}} {{random.integer}}");
    send.composer.set_template(on);
    let form = form();

    let out = send.apply(SendInput::SendPressed, cx(&form));

    let [SendEffect::StartSend(request)] = out.effects.as_slice() else {
        panic!("expected StartSend, got {:?}", out.effects);
    };
    assert_eq!(
        request.template.as_ref().map(|t| t.placeholder_count()),
        placeholders
    );
    assert_eq!(request.event.body, b"{{person.email}} {{random.integer}}");
}

#[test]
fn a_sample_that_fails_to_render_does_not_block_the_press() {
    let mut send = with_template("{{internet.password length=1048576}}x");
    let form = form();

    let out = send.apply(SendInput::SendPressed, cx(&form));

    assert_eq!(out.effects.len(), 1);
}

#[test]
fn a_press_closes_the_placeholder_list_even_when_it_is_ignored() {
    let mut send = with_template("{{person.email}}");
    send.composer.placeholders_open = true;
    let form = form();
    let ignored = PressContext {
        sheet_open: true,
        ..cx(&form)
    };

    let out = send.apply(SendInput::SendPressed, ignored);

    assert!(!send.composer.placeholders_open);
    assert!(out.changed && out.effects.is_empty());
}

#[rstest]
#[case::over_cap_shrinks_to_a_template("{{random.integer}}", Some(1), false)]
#[case::over_cap_shrinks_to_a_broken_template("{{nope}}", None, true)]
fn an_edit_that_brings_an_over_cap_body_under_the_cap_recompiles_it(
    #[case] body: &str,
    #[case] placeholders: Option<usize>,
    #[case] invalid: bool,
) {
    let mut send = with_template(&"x".repeat(BODY_CAP + 1));
    let over_cap = matches!(
        send.composer.template(),
        super::template::TemplateState::OverCap
    );

    send.composer.body.replace_text(body);
    send.composer.refresh_template();

    let state = send.composer.template();
    assert!(over_cap);
    assert_eq!(
        state.template().map(|t| t.placeholder_count()),
        placeholders
    );
    assert_eq!(state.issue().is_some(), invalid);
}

#[test]
fn an_edit_while_the_switch_is_off_compiles_nothing() {
    let mut send = ready();
    send.composer.body.replace_text("{{nope}}");

    send.composer.refresh_template();

    assert!(!send.composer.template().is_on());
    assert!(send.composer.issues(true).is_empty());
}

#[test]
fn regenerate_bumps_the_generation_of_a_compiled_template() {
    let mut send = with_template("{{internet.password}}");

    let rendered = send.composer.regenerate();

    assert!(rendered);
    assert!(matches!(
        send.composer.template(),
        super::template::TemplateState::Ready { generation: 1, .. }
    ));
}

fn sample_text(send: &SendState) -> Option<String> {
    match send.composer.template() {
        TemplateState::Ready { sample, .. } => sample.clone().ok(),
        TemplateState::Off | TemplateState::OverCap | TemplateState::Invalid(_) => None,
    }
}

fn generation(send: &SendState) -> Option<u64> {
    match send.composer.template() {
        TemplateState::Ready { generation, .. } => Some(*generation),
        TemplateState::Off | TemplateState::OverCap | TemplateState::Invalid(_) => None,
    }
}

#[test]
fn the_sample_shows_the_number_the_next_event_gets_and_regenerate_keeps_it() {
    let mut send = ready();
    send.composer.sequence().store(4, Ordering::Relaxed);
    send.composer.body.replace_text("n={{sequence}}");

    send.composer.set_template(true);
    let switched = sample_text(&send);
    send.composer.body.replace_text("m={{sequence}}");
    send.composer.refresh_template();
    let edited = sample_text(&send);
    send.composer.regenerate();

    assert_eq!(switched.as_deref(), Some("n=4"));
    assert_eq!(edited.as_deref(), Some("m=4"));
    assert_eq!(sample_text(&send).as_deref(), Some("m=4"));
    assert_eq!(send.composer.sequence().load(Ordering::Relaxed), 4);
}

#[rstest]
#[case::numbered(Some("{{sequence}}"), Some(4))]
#[case::without_a_sequence(Some("{{person.email}}"), None)]
#[case::switch_off(None, None)]
fn a_press_hands_the_composers_counter_to_the_run(
    #[case] source: Option<&str>,
    #[case] from: Option<u64>,
) {
    let mut send = ready();
    send.composer
        .body
        .replace_text(source.unwrap_or("{{sequence}}"));
    send.composer.set_template(source.is_some());
    send.composer.sequence().store(4, Ordering::Relaxed);

    let out = apply(&mut send, SendInput::SendPressed);

    let [SendEffect::StartSend(request)] = out.effects.as_slice() else {
        panic!("expected StartSend, got {:?}", out.effects);
    };
    assert!(Arc::ptr_eq(&request.sequence, send.composer.sequence()));
    assert_eq!(
        sequence_from(request.template.as_deref(), &request.sequence),
        from
    );
}

#[rstest]
#[case::completed(SendEnd::Completed)]
#[case::cancelled(SendEnd::Cancelled)]
#[case::failed(SendEnd::Failed("throttled".to_owned()))]
fn the_end_of_a_run_renders_a_sequence_sample_with_the_next_number(#[case] end: SendEnd) {
    let mut send = with_template("n={{sequence}}");
    let out = apply(&mut send, SendInput::SendPressed);
    let [SendEffect::StartSend(request)] = out.effects.as_slice() else {
        panic!("expected StartSend, got {:?}", out.effects);
    };
    let run = request.run;
    request.sequence.fetch_add(3, Ordering::Relaxed);

    apply(
        &mut send,
        SendInput::Finished {
            run,
            summary: summary(3, end),
            close_error: None,
        },
    );

    assert_eq!(sample_text(&send).as_deref(), Some("n=3"));
}

#[test]
fn the_end_of_a_started_run_renders_a_sequence_sample_with_the_next_number() {
    let mut send = with_template("n={{sequence}}");
    let run = sending(&mut send, Utc::now());
    send.composer.sequence().fetch_add(2, Ordering::Relaxed);

    apply(
        &mut send,
        SendInput::Finished {
            run,
            summary: summary(2, SendEnd::Completed),
            close_error: None,
        },
    );

    assert_eq!(sample_text(&send).as_deref(), Some("n=2"));
}

#[test]
fn the_end_of_a_run_keeps_the_sample_of_a_template_without_a_sequence() {
    let mut send = with_template("{{internet.password length=64}}");
    let before = sample_text(&send);
    let run = sending(&mut send, Utc::now());

    apply(
        &mut send,
        SendInput::Finished {
            run,
            summary: summary(1, SendEnd::Completed),
            close_error: None,
        },
    );

    assert!(before.is_some());
    assert_eq!(sample_text(&send), before);
    assert_eq!(generation(&send), Some(0));
}

#[test]
fn the_end_of_another_run_renders_no_sample() {
    let mut send = with_template("n={{sequence}}");
    let run = sending(&mut send, Utc::now());

    apply(
        &mut send,
        SendInput::Finished {
            run: run + 1,
            summary: summary(1, SendEnd::Completed),
            close_error: None,
        },
    );

    assert_eq!(generation(&send), Some(0));
}

#[test]
fn properties_and_the_correlation_id_reach_the_event() {
    let mut send = ready();
    send.composer
        .properties
        .replace_text("source = load-test\n\n  \nrun=7\r\nnote=");
    send.composer.correlation_id.replace_text(" trace-1 ");

    let event = send.composer.compose(true).unwrap().event;

    assert_eq!(
        event.properties.into_iter().collect::<Vec<_>>(),
        [
            ("note".to_owned(), String::new()),
            ("run".to_owned(), "7".to_owned()),
            ("source".to_owned(), "load-test".to_owned())
        ]
    );
    assert_eq!(event.correlation_id.as_deref(), Some("trace-1"));
}

#[rstest]
#[case::empty("")]
#[case::blank("   ")]
fn a_blank_correlation_id_and_empty_properties_send_none(#[case] text: &str) {
    let mut send = ready();
    send.composer.correlation_id.replace_text(text);
    send.composer.properties.replace_text(text);

    let event = send.composer.compose(true).unwrap().event;

    assert_eq!(event.correlation_id, None);
    assert!(event.properties.is_empty());
}

#[rstest]
#[case::no_equals("a=b\n\nsecret-text", "Line 3: no '=' (write it as key=value)")]
#[case::no_key("=secret-text", "Line 1: no key before '='")]
#[case::repeated(
    "run=secret-1\nx=y\nrun=secret-2",
    "Line 3: the key 'run' is given twice"
)]
fn a_bad_properties_line_is_its_issue_naming_the_line(#[case] text: &str, #[case] expected: &str) {
    let mut send = ready();
    send.composer.properties.replace_text(text);

    let issues = send.composer.issues(true);

    assert_eq!(issues.properties.as_deref(), Some(expected));
    assert_eq!(issues.first(), Some(SendField::Properties));
    assert_eq!(issues.len(), 1);
}

#[test]
fn properties_over_the_cap_are_refused_before_any_line_is_parsed() {
    let mut send = ready();
    let text = format!("ab\n{}", "x".repeat(super::compose::PROPERTIES_CAP));
    send.composer.properties.replace_text(text);

    let issues = send.composer.issues(true);

    assert_eq!(
        issues.properties.as_deref(),
        Some("Properties are over 4 KB; remove some")
    );
}

#[test]
fn properties_at_the_cap_are_parsed() {
    let mut send = ready();
    let key = "k".repeat(super::compose::PROPERTIES_CAP - 1);
    send.composer.properties.replace_text(format!("{key}="));

    assert!(send.composer.issues(true).is_empty());
}

#[test]
fn a_press_with_a_properties_issue_asks_for_the_properties_field_and_sends_nothing() {
    let mut send = ready();
    send.composer.properties.replace_text("no equals");

    let out = apply(&mut send, SendInput::SendPressed);

    assert!(out.effects.is_empty() && out.changed);
    assert_eq!(send.shown_issues().len(), 1);
    assert_eq!(send.take_focus(), Some(SendField::Properties));
}

#[test]
fn debug_of_the_composer_and_its_checks_hides_metadata_text() {
    let mut send = ready();
    send.composer
        .properties
        .replace_text("customer=zyxwv\nrun=7");
    send.composer.correlation_id.replace_text("trace-abc");
    let validated = send.composer.validate(true).unwrap();

    let rendered = format!("{:?} {validated:?}", send.composer);

    for hidden in ["customer", "zyxwv", "trace-abc"] {
        assert!(!rendered.contains(hidden), "{hidden} in {rendered}");
    }
    assert!(rendered.contains("property_lines: 2"), "{rendered}");
    assert!(rendered.contains("property_count: 2"), "{rendered}");
    assert!(rendered.contains("correlation_id: \"set\""), "{rendered}");
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
