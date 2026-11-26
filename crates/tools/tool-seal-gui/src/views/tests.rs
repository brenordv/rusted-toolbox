use super::*;
use crate::design::{TEXT_MIN_LINES, WINDOW_MIN, WINDOW_SIZE};
use crate::state::notes::{Note, RunningJob};
use crate::state::recipients::{EntryId, Picker};
use crate::test_support::{Recording, recording_state, state_with_book};
use crate::worker::{JobEvent, JobOutcome, JobRequest};
use common_gui_rune::test_harness::{
    self, a11y_node, a11y_nodes, answer_dialog, bottom, clipboard_text, command, command_mods,
    focus_is_inside, has_key, has_text, is_disabled, press, press_key, rect, runtime_with_keys,
    settle, type_into, use_memory_clipboard,
};
use common_gui_rune::tokens::TYPE_MONO;
use rune::{FileDialogKind, Key, KeyEvent, Runtime};
use serde_json::Value;
use shared_crypto::Direction;
use std::path::PathBuf;
use std::rc::Rc;

const ARMOR: &str = "-----BEGIN AGE ENCRYPTED FILE-----\nYWdl\n-----END AGE ENCRYPTED FILE-----";

fn window_at(state: AppState, size: (f32, f32)) -> Runtime<AppState> {
    runtime_with_keys(state, view, window_key, size)
}

fn window(book: &[&str], size: (f32, f32)) -> (Runtime<AppState>, Rc<Recording>) {
    let (state, services) = recording_state(state_with_book(book));
    (window_at(state, size), services)
}

fn deliver(rt: &mut Runtime<AppState>, input: Input) {
    test_harness::deliver(rt, move |s, tx| dispatch(s, tx, input));
}

fn first_id(rt: &Runtime<AppState>) -> EntryId {
    rt.state.book.entries()[0].id
}

fn node(rt: &Runtime<AppState>, key: &str) -> Value {
    let tree = rt.accessibility_json();
    a11y_node(&tree, key)
        .cloned()
        .unwrap_or_else(|| panic!("no accessibility node keyed {key}"))
}

fn states_of(node: &Value) -> Vec<String> {
    node["states"]
        .as_array()
        .map(|states| {
            states
                .iter()
                .filter_map(|state| state.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether a wheel over the Text tab's body moves the input box.
fn body_scrolls(rt: &mut Runtime<AppState>) -> bool {
    let before = rect(rt, INPUT_KEY)[1];
    let [x, y, width, _] = rect(rt, TEXT_SCROLL_KEY);
    rt.wheel(x + width / 2.0, y + 4.0, -240.0);
    settle(rt);
    rect(rt, INPUT_KEY)[1] < before
}

fn mono_lines(lines: f32) -> f32 {
    lines * TYPE_MONO.line_px()
}

#[test]
fn v1_typing_checking_and_mod_enter_seal_then_copy_the_output() {
    let (mut rt, services) = window(&["Alice"], WINDOW_MIN);
    let clipboard = use_memory_clipboard(&mut rt);
    type_into(&mut rt, INPUT_KEY, "a secret note");
    let alice = first_id(&rt);
    press(&mut rt, &recipient_key(Picker::Text, alice));
    press(&mut rt, INPUT_KEY);

    command(&mut rt, Key::Enter);
    let runs = services.take_runs();
    deliver(
        &mut rt,
        Input::Job(JobEvent::Finished(JobOutcome::Text(ARMOR.to_owned()))),
    );
    press(&mut rt, COPY_OUTPUT_KEY);

    assert!(matches!(
        runs.as_slice(),
        [JobRequest::EncryptText { text, recipients }]
            if text == "a secret note" && recipients.len() == 1
    ));
    assert!(has_text(&rt, "YWdl"));
    assert!(bottom(&rt, OUTPUT_KEY) <= WINDOW_MIN.1);
    assert_eq!(clipboard_text(&clipboard).as_deref(), Some(ARMOR));
    assert!(has_text(&rt, "Copied the output."));
}

#[test]
fn v2_return_in_the_input_types_a_line_break_and_starts_nothing() {
    let (mut rt, services) = window(&["Alice"], WINDOW_SIZE);
    type_into(&mut rt, INPUT_KEY, "one");

    press_key(&mut rt, Key::Enter);

    assert_eq!(rt.state.text.input.text, "one\n");
    assert_eq!(services.run_count(), 0);
}

#[test]
fn v3_mod_2_and_mod_3_switch_tabs_and_mod_1_comes_back_to_the_input() {
    let (mut rt, _) = window(&["Alice"], WINDOW_SIZE);
    type_into(&mut rt, INPUT_KEY, "kept");

    command(&mut rt, Key::Command('2'));
    let on_files = has_key(&rt, FILES_TAB_KEY);
    command(&mut rt, Key::Command('3'));
    let on_keys = has_key(&rt, KEYS_TAB_KEY);
    command(&mut rt, Key::Command('1'));

    assert!(on_files && on_keys);
    assert!(has_key(&rt, TEXT_TAB_KEY));
    assert_eq!(rt.state.text.input.text, "kept");
    assert!(focus_is_inside(&rt, INPUT_KEY));
}

#[test]
fn v4_while_a_job_runs_the_seal_button_is_busy_and_a_second_start_asks_nothing() {
    let (mut rt, services) = window(&["Alice"], WINDOW_SIZE);
    type_into(&mut rt, INPUT_KEY, "hello");
    let alice = first_id(&rt);
    press(&mut rt, &recipient_key(Picker::Text, alice));
    press(&mut rt, TEXT_SUBMIT_KEY);

    press(&mut rt, INPUT_KEY);
    command(&mut rt, Key::Enter);

    assert_eq!(services.run_count(), 1);
    assert_eq!(
        rt.state.work,
        Work::Running(RunningJob::Text(Direction::Encrypt))
    );
    assert!(is_disabled(&rt, TEXT_SUBMIT_KEY));
    let tree = rt.accessibility_json();
    let segments: Vec<&Value> = a11y_nodes(&tree)
        .into_iter()
        .filter(|node| node["name"] == "Seal" || node["name"] == "Unseal")
        .collect();
    assert!(
        segments
            .iter()
            .all(|segment| states_of(segment).iter().any(|state| state == "disabled")),
        "{segments:?}"
    );
}

#[test]
fn v5_a_press_with_a_blank_input_marks_it_and_moves_focus_to_it() {
    let (mut rt, services) = window(&["Alice"], WINDOW_SIZE);

    press(&mut rt, TEXT_SUBMIT_KEY);

    assert_eq!(services.run_count(), 0);
    assert!(!is_disabled(&rt, TEXT_SUBMIT_KEY));
    assert!(focus_is_inside(&rt, INPUT_KEY));
    assert!(has_text(&rt, "Enter some text first."));
    let tree = rt.accessibility_json();
    let input = a11y_nodes(&tree)
        .into_iter()
        .find(|node| node["name"] == "Input")
        .cloned()
        .unwrap();
    assert!(
        states_of(&input).iter().any(|state| state == "invalid"),
        "{input}"
    );
}

#[test]
fn v6_choose_identity_file_opens_one_dialog_and_its_answer_sets_the_row() {
    let (mut rt, _) = window(&["Alice"], WINDOW_SIZE);
    deliver(
        &mut rt,
        Input::Text(crate::state::text::TextInput::DirectionPicked(
            Direction::Decrypt,
        )),
    );

    press(&mut rt, CHOOSE_IDENTITY_KEY);
    let title = rt
        .file_dialog()
        .map(|(_, options)| options.title.clone())
        .unwrap_or_default();
    let disabled_while_open = is_disabled(&rt, CHOOSE_IDENTITY_KEY);
    answer_dialog(
        &mut rt,
        FileDialogKind::Open,
        Some(&PathBuf::from("my-identity.txt")),
    );

    assert_eq!(title, "Choose identity file");
    assert!(disabled_while_open);
    assert!(has_text(&rt, "my-identity.txt"));
    assert!(!is_disabled(&rt, CHOOSE_IDENTITY_KEY));
}

#[test]
fn v7_every_focusable_node_has_a_name() {
    let (mut rt, _) = window(&["Alice", "Bea"], WINDOW_SIZE);
    rt.state.text.output = ARMOR.to_owned();
    settle(&mut rt);

    let tree = rt.accessibility_json();
    let unnamed: Vec<&Value> = a11y_nodes(&tree)
        .into_iter()
        .filter(|node| {
            states_of(node).iter().any(|state| state == "focusable")
                && node["name"].as_str().is_none_or(str::is_empty)
        })
        .collect();

    assert!(unnamed.is_empty(), "{unnamed:?}");
}

#[test]
fn v7_the_status_is_a_polite_status_and_an_empty_one_is_not_mounted() {
    let (mut rt, _) = window(&["Alice"], WINDOW_SIZE);
    let height_without = rect(&rt, TEXT_ACTIONS_KEY)[3];
    let mounted_without = has_key(&rt, TEXT_STATUS_KEY);

    press(&mut rt, TEXT_SUBMIT_KEY);
    let status = node(&rt, TEXT_STATUS_KEY);

    assert!(!mounted_without);
    assert_eq!(status["role"], "status");
    assert_eq!(status["name"], "Enter some text first.");
    assert!(
        states_of(&status)
            .iter()
            .any(|state| state == "live-polite")
    );
    assert_eq!(rect(&rt, TEXT_ACTIONS_KEY)[3], height_without);
}

#[test]
fn v7_the_output_box_and_the_recipients_are_named_and_a_key_is_described_whole() {
    let (rt, _) = window(&["Alice"], WINDOW_SIZE);
    let alice = &rt.state.book.entries()[0];

    let output = node(&rt, OUTPUT_KEY);
    let recipients = node(&rt, RECIPIENTS_KEY);
    let checkbox = node(&rt, &recipient_key(Picker::Text, alice.id));

    assert_eq!(output["name"], "Output");
    assert!(output["role"].as_str().is_some());
    assert_eq!(recipients["role"], "group");
    assert_eq!(recipients["name"], "Recipients");
    assert_eq!(checkbox["name"], "Alice");
    assert_eq!(checkbox["description"], alice.public_key.as_str());
}

#[test]
fn v9_at_rest_with_twenty_recipients_the_smallest_window_does_not_scroll() {
    let labels: Vec<String> = (0..20).map(|index| format!("Recipient {index}")).collect();
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let (mut rt, _) = window(&labels, WINDOW_MIN);

    let input_height = rect(&rt, INPUT_KEY)[3];
    let output_height = rect(&rt, OUTPUT_KEY)[3];

    assert!(!body_scrolls(&mut rt));
    assert!(input_height >= mono_lines(TEXT_MIN_LINES), "{input_height}");
    assert!(
        output_height >= mono_lines(TEXT_MIN_LINES),
        "{output_height}"
    );
    assert!(bottom(&rt, TEXT_SUBMIT_KEY) <= WINDOW_MIN.1);
}

#[test]
fn v9_at_the_default_size_the_boxes_take_the_leftover_height() {
    let (rt, _) = window(&["Alice"], WINDOW_SIZE);

    let input_height = rect(&rt, INPUT_KEY)[3];
    let output_height = rect(&rt, OUTPUT_KEY)[3];

    assert!(
        input_height > mono_lines(TEXT_MIN_LINES) + 40.0,
        "{input_height}"
    );
    assert!(
        output_height > mono_lines(TEXT_MIN_LINES) + 40.0,
        "{output_height}"
    );
}

#[test]
fn v9_with_a_long_failure_and_the_stopped_caption_seal_stays_on_screen_and_the_body_scrolls() {
    let (mut rt, _) = window(&["Alice"], WINDOW_MIN);
    deliver(&mut rt, Input::Stopped);
    rt.state
        .notes_mut_for_tests()
        .set(Tab::Text, Note::Failure("x".repeat(2_000)));
    settle(&mut rt);
    let output_bottom_before = bottom(&rt, OUTPUT_KEY);

    let scrolled = body_scrolls(&mut rt);
    for _ in 0..10 {
        let [x, y, width, _] = rect(&rt, TEXT_SCROLL_KEY);
        rt.wheel(x + width / 2.0, y + 4.0, -240.0);
    }
    settle(&mut rt);
    let [_, scroll_y, _, scroll_height] = rect(&rt, TEXT_SCROLL_KEY);

    assert!(has_key(&rt, STOPPED_KEY));
    assert!(bottom(&rt, TEXT_SUBMIT_KEY) <= WINDOW_MIN.1);
    assert!(rect(&rt, TEXT_SUBMIT_KEY)[1] >= 0.0);
    assert!(scrolled || output_bottom_before <= scroll_y + scroll_height);
    assert!(bottom(&rt, OUTPUT_KEY) <= scroll_y + scroll_height + 0.5);
}

#[test]
fn v10_once_the_worker_stopped_every_tab_shows_the_caption_and_a_copy_still_says_so() {
    let (mut rt, _) = window(&["Alice"], WINDOW_SIZE);
    rt.state.text.output = ARMOR.to_owned();
    deliver(&mut rt, Input::Stopped);
    let text_status_empty = !has_key(&rt, TEXT_STATUS_KEY);
    let mut on_every_tab = Vec::new();
    for chord in ['2', '3', '1'] {
        command(&mut rt, Key::Command(chord));
        on_every_tab.push(has_key(&rt, STOPPED_KEY));
    }

    press(&mut rt, COPY_OUTPUT_KEY);

    assert!(text_status_empty);
    assert_eq!(on_every_tab, [true, true, true]);
    assert!(has_text(&rt, "Copied the output."));
    assert!(is_disabled(&rt, TEXT_SUBMIT_KEY));
}

#[test]
fn w10_w17_a_start_the_worker_refuses_ends_idle_in_one_dispatch_with_the_failure() {
    let (mut rt, services) = window(&["Alice"], WINDOW_SIZE);
    services.refuse.set(true);
    type_into(&mut rt, INPUT_KEY, "hello");
    let alice = first_id(&rt);
    press(&mut rt, &recipient_key(Picker::Text, alice));

    deliver(
        &mut rt,
        Input::Text(crate::state::text::TextInput::StartPressed),
    );

    assert_eq!(rt.state.work, Work::Idle);
    assert_eq!(
        rt.state.note(Tab::Text),
        Some(&Note::Failure(crate::worker::WORKER_GONE.to_owned()))
    );
    assert!(has_text(&rt, "The background worker has stopped"));
}

#[test]
fn a_start_in_a_window_with_no_services_fails_at_once() {
    let mut state = state_with_book(&["Alice"]);
    state.text.input = rune::TextEdit::new("hello");
    let alice = state.book.entries()[0].id;
    state.text.picker.toggle(alice);
    let mut rt = window_at(state, WINDOW_SIZE);

    deliver(
        &mut rt,
        Input::Text(crate::state::text::TextInput::StartPressed),
    );

    assert_eq!(rt.state.work, Work::Idle);
    assert_eq!(
        rt.state.note(Tab::Text),
        Some(&Note::Failure(crate::worker::WORKER_GONE.to_owned()))
    );
}

#[test]
fn a_held_tab_chord_switches_nothing() {
    let (mut rt, _) = window(&["Alice"], WINDOW_SIZE);
    press(&mut rt, INPUT_KEY);

    rt.repeat_key(KeyEvent {
        key: Key::Command('2'),
        mods: command_mods(),
    });
    settle(&mut rt);

    assert!(has_key(&rt, TEXT_TAB_KEY));
}

#[test]
fn an_empty_book_says_where_recipients_come_from() {
    let (rt, _) = window(&[], WINDOW_SIZE);

    assert!(has_text(
        &rt,
        "No saved recipients yet. Add them in the Keys tab."
    ));
}

#[test]
fn a_bad_other_key_shows_its_issue_under_the_field() {
    let (mut rt, _) = window(&[], WINDOW_SIZE);

    type_into(&mut rt, OTHER_KEY_KEY, "not-a-key");

    assert!(has_key(&rt, &format!("{OTHER_KEY_KEY}-issue")));
}

#[test]
fn a_label_with_control_characters_shows_cleaned() {
    let (rt, _) = window(&["Ali\u{202E}ce\tB"], WINDOW_SIZE);

    assert!(!has_text(&rt, "\u{202E}"));
    assert!(has_text(&rt, "Ali ce B"));
}

#[test]
fn the_window_draws_in_the_appearance_it_is_told() {
    let (mut rt, _) = window(&[], WINDOW_SIZE);

    let first = rt.update_if(|s, _| set_appearance(s, false));
    let again = rt.update_if(|s, _| set_appearance(s, false));

    assert!(first);
    assert!(!again);
}

#[test]
fn closing_the_window_shuts_the_post_gate() {
    let gate = crate::services::PostGate::default();
    let mut state = AppState::detached();
    state.gate = Some(gate.clone());
    let mut rt = window_at(state, WINDOW_SIZE);
    let forwarded = std::cell::Cell::new(false);

    rt.update_if(|s, tx| {
        on_close(s, tx);
        true
    });
    gate.forward(|| forwarded.set(true));

    assert!(!forwarded.get());
}
