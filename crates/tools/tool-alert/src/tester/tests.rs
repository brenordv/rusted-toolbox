use super::*;
use crate::cli_utils::CliArgs;
use crate::design::TESTER_SIZE;
use clap::Parser;
use common_gui_rune::test_harness::{
    a11y_node, command, element, has_key, has_text, press, press_key, runtime_at, settle,
};
use rstest::rstest;
use rune::{Key, Runtime};
use std::cell::RefCell;
use std::rc::Rc;

const START_FAILURE: &str = "the program is missing";
const FIELD_KEYS: [&str; 4] = [TITLE_KEY, BODY_KEY, SOUND_KEY, TEST_KEY];

type Starts = Rc<RefCell<Vec<Vec<OsString>>>>;

fn seed(title: &str, body: &str, sound: &str) -> TesterSeed {
    TesterSeed {
        title: title.to_owned(),
        body: body.to_owned(),
        sound: sound.to_owned(),
    }
}

fn recording(seed: &TesterSeed) -> (TesterState, Starts) {
    let starts = Starts::default();
    let record = Rc::clone(&starts);
    let state = TesterState::new(
        seed,
        Box::new(move |arguments| {
            record.borrow_mut().push(arguments.to_vec());
            Ok(())
        }),
    );
    (state, starts)
}

fn runtime(state: TesterState) -> Runtime<TesterState> {
    let mut rt = runtime_at(state, view, TESTER_SIZE);
    rt.on_key = Some(window_key);
    rt
}

fn focused_field(rt: &Runtime<TesterState>) -> Option<&'static str> {
    let focused = rt.elements().into_iter().find(|element| element.focused)?;
    FIELD_KEYS.into_iter().find(|key| {
        let field = element(rt, key).path;
        focused.path == field || focused.path.starts_with(&format!("{field}/"))
    })
}

fn parsed_back(arguments: &[OsString]) -> (Option<String>, Option<String>, Option<String>) {
    let command_line = std::iter::once(OsString::from("alert")).chain(arguments.iter().cloned());
    let args = CliArgs::try_parse_from(command_line)
        .unwrap()
        .into_alert_args();
    (args.title, args.body, args.sound)
}

#[rstest]
#[case::body_only("", "Build finished", "", &["--", "Build finished"])]
#[case::with_a_title("Done", "Build finished", "", &["--title=Done", "--", "Build finished"])]
#[case::with_a_sound("", "Build finished", "ping.wav", &["--sound=ping.wav", "--", "Build finished"])]
#[case::a_body_starting_with_a_dash("", "--gui is a flag", "", &["--", "--gui is a flag"])]
#[case::a_title_starting_with_a_dash("-t", "Body", "", &["--title=-t", "--", "Body"])]
#[case::a_sound_starting_with_a_dash("", "Body", "--gui", &["--sound=--gui", "--", "Body"])]
#[case::a_blank_title_and_sound("  ", "Body", " ", &["--", "Body"])]
#[case::everything("Done", "All 12 passed", "ping.wav", &["--title=Done", "--sound=ping.wav", "--", "All 12 passed"])]
fn alert_arguments_keep_every_value_out_of_the_flags(
    #[case] title: &str,
    #[case] body: &str,
    #[case] sound: &str,
    #[case] expected: &[&str],
) {
    let arguments = alert_arguments(title, body, sound);

    let expected: Vec<OsString> = expected.iter().map(OsString::from).collect();
    assert_eq!(arguments, expected);
    let not_blank = |value: &str| (!value.trim().is_empty()).then(|| value.to_owned());
    assert_eq!(
        parsed_back(&arguments),
        (not_blank(title), Some(body.to_owned()), not_blank(sound))
    );
}

#[test]
fn start_alert_with_a_missing_program_is_an_error() {
    let folder = tempfile::tempdir().unwrap();
    let missing = folder.path().join("no-such-alert");

    let result = start_alert(&missing, &[OsString::from("--help")]);

    let error = format!("{:#}", result.unwrap_err());
    assert!(
        error.starts_with("Failed to start the alert program"),
        "{error}"
    );
}

#[test]
fn start_alert_starts_the_program_it_is_given() {
    let this_test_program = std::env::current_exe().unwrap();

    let result = start_alert(&this_test_program, &[OsString::from("--list")]);

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn test_with_a_blank_body_shows_the_issue_and_starts_nothing() {
    let (state, starts) = recording(&seed("Done", "   ", "ping.wav"));
    let mut rt = runtime(state);

    press(&mut rt, TEST_KEY);

    let tree = rt.accessibility_json();
    let issue = a11y_node(&tree, ISSUE_KEY).unwrap();
    assert_eq!(issue["name"], BLANK_BODY, "{issue}");
    assert_eq!(issue["role"], "status", "{issue}");
    assert!(starts.borrow().is_empty());
}

#[test]
fn test_with_all_three_fields_starts_one_alert_with_their_arguments() {
    let (state, starts) = recording(&seed("Done", "All 12 passed", "ping.wav"));
    let mut rt = runtime(state);

    press(&mut rt, TEST_KEY);

    assert_eq!(
        *starts.borrow(),
        vec![alert_arguments("Done", "All 12 passed", "ping.wav")]
    );
    assert!(!has_key(&rt, ISSUE_KEY));
}

#[test]
fn a_start_that_fails_shows_its_text_in_the_status_line() {
    let state = TesterState::new(
        &seed("", "Body", ""),
        Box::new(|_| anyhow::bail!(START_FAILURE)),
    );
    let mut rt = runtime(state);

    press(&mut rt, TEST_KEY);

    assert!(has_text(
        &rt,
        &format!("Couldn't start the alert: {START_FAILURE}")
    ));
    assert_eq!(
        rt.state.issue(),
        Some(format!("Couldn't start the alert: {START_FAILURE}").as_str())
    );
}

#[test]
fn a_good_start_after_a_failed_one_clears_the_issue() {
    let (mut state, starts) = recording(&seed("", "", ""));
    state.test();
    state.body = TextEdit::new("Body");

    state.test();

    assert_eq!(state.issue(), None);
    assert_eq!(starts.borrow().len(), 1);
}

#[test]
fn the_detached_form_reports_that_it_can_start_nothing() {
    let mut state = TesterState::detached();
    state.body = TextEdit::new("Body");

    state.test();

    assert_eq!(
        state.issue(),
        Some("Couldn't start the alert: The tester is not set up")
    );
}

#[test]
fn tab_moves_through_title_body_sound_file_and_test() {
    let (state, _) = recording(&seed("", "", ""));
    let mut rt = runtime(state);
    let mut order = Vec::new();

    for _ in FIELD_KEYS {
        press_key(&mut rt, Key::Tab);
        order.push(focused_field(&rt));
    }

    assert_eq!(order, FIELD_KEYS.map(Some));
}

#[rstest]
#[case::from_the_body(Some(BODY_KEY))]
#[case::from_the_title(Some(TITLE_KEY))]
#[case::from_the_sound_file(Some(SOUND_KEY))]
#[case::with_nothing_focused(None)]
fn mod_enter_presses_test(#[case] focus: Option<&str>) {
    let (state, starts) = recording(&seed("", "Body", ""));
    let mut rt = runtime(state);
    if let Some(field) = focus {
        press(&mut rt, field);
    }

    command(&mut rt, Key::Enter);

    assert_eq!(focused_field(&rt), focus);
    assert_eq!(*starts.borrow(), vec![alert_arguments("", "Body", "")]);
}

#[test]
fn return_in_the_body_adds_a_line_and_starts_nothing() {
    let (state, starts) = recording(&seed("", "", ""));
    let mut rt = runtime(state);
    press(&mut rt, BODY_KEY);
    rt.paste("one");

    press_key(&mut rt, Key::Enter);

    assert_eq!(rt.state.body_text(), "one\n");
    assert!(starts.borrow().is_empty());
}

#[test]
fn editing_the_body_clears_the_issue() {
    let (state, _) = recording(&seed("", "", ""));
    let mut rt = runtime(state);
    press(&mut rt, TEST_KEY);
    let shown = has_key(&rt, ISSUE_KEY);
    press(&mut rt, BODY_KEY);

    rt.paste("Body");
    settle(&mut rt);

    assert!(shown);
    assert!(!has_key(&rt, ISSUE_KEY));
}

#[test]
fn a_form_built_from_command_line_values_shows_them_in_the_three_fields() {
    let (state, _) = recording(&seed("Done", "All 12 passed", "ping.wav"));
    let rt = runtime(state);

    assert!(has_text(&rt, "Done"));
    assert!(has_text(&rt, "All 12 passed"));
    assert!(has_text(&rt, "ping.wav"));
}

#[test]
fn the_appearance_hook_reports_a_change_once() {
    let (mut state, _) = recording(&seed("", "", ""));

    let first = set_appearance(&mut state, true);
    let again = set_appearance(&mut state, true);

    assert!(first);
    assert!(!again);
}

#[test]
fn the_window_first_showing_changes_nothing() {
    let (state, _) = recording(&seed("", "", ""));
    let mut rt = runtime(state);

    let changed = rt.update_if(shown);

    assert!(!changed);
}
