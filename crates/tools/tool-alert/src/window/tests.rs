use super::*;
use crate::design::ALERT_SIZE;
use common_gui_rune::test_harness::{
    a11y_node, bottom, element, focused_key, has_key, has_text, press, press_key, rect,
    runtime_with_keys, settle, text_visible,
};
use common_gui_rune::tokens::TYPE_CAPTION;
use rstest::rstest;
use rune::Runtime;

const BODY: &str = "The build finished and every test passed.";
const TITLE: &str = "Build finished";
const SOUND_ISSUE: &str = "No sound output device could be opened";
const LONG_AGO: Duration = Duration::from_secs(2);

fn spec(title: Option<&str>, body: &str) -> AlertSpec {
    AlertSpec {
        title: title.map(str::to_owned),
        body: body.to_owned(),
        sound: None,
    }
}

fn not_yet_shown(title: Option<&str>, body: &str) -> AlertState {
    AlertState::new(&spec(title, body), None)
}

fn shown_long_ago(title: Option<&str>, body: &str) -> AlertState {
    let mut state = not_yet_shown(title, body);
    state.mark_shown(Instant::now().checked_sub(LONG_AGO).unwrap());
    state
}

fn runtime(state: AlertState) -> Runtime<AlertState> {
    runtime_with_keys(state, view, window_key, ALERT_SIZE)
}

fn key(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        mods: Mods::default(),
    }
}

#[rstest]
#[case::pointer_before_the_window_shows(CloseEvent::Pointer, false, None, Some(ClosedBy::Button))]
#[case::pointer_at_once(CloseEvent::Pointer, false, Some(0), Some(ClosedBy::Button))]
#[case::pointer_later(CloseEvent::Pointer, false, Some(5_000), Some(ClosedBy::Button))]
#[case::return_at_the_guard(CloseEvent::ButtonKey, false, Some(1_000), Some(ClosedBy::Key))]
#[case::return_later(CloseEvent::ButtonKey, false, Some(60_000), Some(ClosedBy::Key))]
#[case::return_just_before_the_guard(CloseEvent::ButtonKey, false, Some(999), None)]
#[case::return_before_the_window_shows(CloseEvent::ButtonKey, false, None, None)]
#[case::return_held(CloseEvent::ButtonKey, true, Some(60_000), None)]
#[case::escape_at_the_guard(CloseEvent::Escape, false, Some(1_000), Some(ClosedBy::Key))]
#[case::escape_just_before_the_guard(CloseEvent::Escape, false, Some(999), None)]
#[case::escape_before_the_window_shows(CloseEvent::Escape, false, None, None)]
#[case::escape_held(CloseEvent::Escape, true, Some(60_000), None)]
#[case::another_key(CloseEvent::OtherKey, false, Some(60_000), None)]
#[case::another_key_held(CloseEvent::OtherKey, true, Some(60_000), None)]
fn closes_lets_a_pointer_through_and_guards_the_keys(
    #[case] event: CloseEvent,
    #[case] repeat: bool,
    #[case] since_shown_ms: Option<u64>,
    #[case] expected: Option<ClosedBy>,
) {
    let closed = closes(event, repeat, since_shown_ms.map(Duration::from_millis));

    assert_eq!(closed, expected);
}

#[rstest]
#[case::button(ClosedBy::Button, "button")]
#[case::key(ClosedBy::Key, "key")]
#[case::window(ClosedBy::Window, "window")]
fn each_way_of_closing_has_its_log_word(#[case] closed_by: ClosedBy, #[case] word: &str) {
    assert_eq!(closed_by.as_str(), word);
}

#[test]
fn the_detached_alert_is_an_untitled_dialog_with_close() {
    let rt = runtime(AlertState::detached());

    let tree = rt.accessibility_json();
    let dialog = a11y_node(&tree, ROOT_KEY).unwrap();

    assert_eq!(dialog["name"], DEFAULT_DIALOG_NAME, "{dialog}");
    assert!(!has_key(&rt, TITLE_KEY));
    assert!(has_key(&rt, CLOSE_KEY));
}

#[test]
fn the_os_title_is_the_tool_name_when_the_alert_has_no_title() {
    assert_eq!(os_title(None), "alert");
}

#[test]
fn the_os_title_carries_the_alerts_title() {
    assert_eq!(os_title(Some("Done")), "alert: Done");
}

#[test]
fn the_os_title_cleans_and_cuts_a_long_title_with_a_line_break() {
    let long = format!("Done\n{}", "x".repeat(300));

    let title = os_title(Some(&long));

    assert!(title.starts_with("alert: Done x"), "{title}");
    assert!(!title.contains('\n'), "{title}");
    assert_eq!(title.chars().count(), "alert: ".len() + 141, "{title}");
    assert!(title.ends_with('…'), "{title}");
}

#[test]
fn the_first_close_is_the_one_recorded() {
    let mut state = shown_long_ago(None, BODY);
    let now = Instant::now();

    let first = state.close(CloseEvent::Escape, false, now);
    let second = state.close(CloseEvent::Pointer, false, now);

    assert!(first);
    assert!(!second);
    assert_eq!(state.closed_by(), Some(ClosedBy::Key));
    assert_eq!(state.window_closed(), ClosedBy::Key);
}

#[test]
fn the_windows_own_close_control_is_recorded_when_nothing_else_closed_it() {
    let mut state = shown_long_ago(None, BODY);

    assert_eq!(state.window_closed(), ClosedBy::Window);
    assert_eq!(state.closed_by(), Some(ClosedBy::Window));
}

#[test]
fn the_sound_starts_once_when_the_window_first_shows() {
    let (starts, started) = std::sync::mpsc::channel();
    let start: Box<dyn FnOnce()> = Box::new(move || starts.send(()).unwrap());
    let mut state = AlertState::new(&spec(None, BODY), Some(start));
    let first_seen = Instant::now();

    state.mark_shown(first_seen);
    state.mark_shown(first_seen + LONG_AGO);

    assert_eq!(started.try_iter().count(), 1);
    assert_eq!(state.shown_at(), Some(first_seen));
}

#[rstest]
#[case::dark(true)]
#[case::light(false)]
fn the_title_shows_above_the_body_with_the_footer_note(#[case] dark: bool) {
    let mut rt = runtime(shown_long_ago(Some(TITLE), BODY));
    rt.update_if(move |state, _| set_appearance(state, dark));
    settle(&mut rt);

    assert!(text_visible(&rt, TITLE));
    assert!(text_visible(&rt, BODY));
    assert!(text_visible(&rt, FOOTER_NOTE));
    assert!(bottom(&rt, TITLE_KEY) <= rect(&rt, BODY_KEY)[1]);
}

#[test]
fn an_alert_without_a_title_mounts_no_title_row() {
    let rt = runtime(shown_long_ago(None, BODY));

    assert!(!has_key(&rt, TITLE_KEY));
    assert!(has_text(&rt, BODY));
}

#[rstest]
#[case::titled(Some(TITLE), TITLE)]
#[case::untitled(None, DEFAULT_DIALOG_NAME)]
fn the_root_is_a_dialog_named_by_the_title_and_described_by_the_body(
    #[case] title: Option<&str>,
    #[case] name: &str,
) {
    let rt = runtime(shown_long_ago(title, BODY));

    let tree = rt.accessibility_json();
    let dialog = a11y_node(&tree, ROOT_KEY).unwrap();

    assert_eq!(dialog["role"], "dialog", "{dialog}");
    assert_eq!(dialog["name"], name, "{dialog}");
    assert_eq!(dialog["description"], BODY, "{dialog}");
}

#[test]
fn showing_puts_focus_on_close_with_no_ring() {
    let mut rt = runtime(shown_long_ago(None, BODY));

    rt.update_if(shown);
    settle(&mut rt);

    assert_eq!(focused_key(&rt).as_deref(), Some(CLOSE_KEY));
    assert!(!element(&rt, CLOSE_KEY).focus_visible);
}

#[test]
fn return_presses_close_as_the_default_button_with_nothing_focused() {
    let mut rt = runtime(shown_long_ago(None, BODY));

    press_key(&mut rt, Key::Enter);

    assert_eq!(focused_key(&rt), None);
    assert_eq!(rt.state.closed_by(), Some(ClosedBy::Key));
}

#[rstest]
#[case::shown(shown_long_ago(None, BODY))]
#[case::not_yet_shown(not_yet_shown(None, BODY))]
fn a_pointer_press_on_close_closes_at_once(#[case] state: AlertState) {
    let mut rt = runtime(state);

    press(&mut rt, CLOSE_KEY);

    assert_eq!(rt.state.closed_by(), Some(ClosedBy::Button));
}

#[rstest]
#[case::enter(Key::Enter)]
#[case::escape(Key::Escape)]
fn a_key_before_the_window_was_seen_closes_nothing(#[case] pressed: Key) {
    let mut rt = runtime(not_yet_shown(None, BODY));

    press_key(&mut rt, pressed);

    assert_eq!(rt.state.closed_by(), None);
}

#[rstest]
#[case::enter(Key::Enter)]
#[case::escape(Key::Escape)]
fn a_key_well_after_the_window_was_seen_closes(#[case] pressed: Key) {
    let mut rt = runtime(shown_long_ago(None, BODY));

    press_key(&mut rt, pressed);

    assert_eq!(rt.state.closed_by(), Some(ClosedBy::Key));
}

#[test]
fn space_on_the_focused_close_button_is_a_key_and_waits_for_the_guard() {
    let mut state = not_yet_shown(None, BODY);
    state.mark_shown(Instant::now());
    let mut rt = runtime(state);
    rt.update_if(|_, tx| {
        tx.focus(CLOSE_KEY, false);
        true
    });
    settle(&mut rt);

    press_key(&mut rt, Key::Text(" ".into()));

    assert_eq!(focused_key(&rt).as_deref(), Some(CLOSE_KEY));
    assert_eq!(rt.state.closed_by(), None);
}

#[rstest]
#[case::enter(Key::Enter)]
#[case::escape(Key::Escape)]
fn a_held_key_never_closes(#[case] held: Key) {
    let mut rt = runtime(shown_long_ago(None, BODY));

    rt.repeat_key(key(held));
    settle(&mut rt);

    assert_eq!(rt.state.closed_by(), None);
}

#[test]
fn another_key_closes_nothing() {
    let mut rt = runtime(shown_long_ago(None, BODY));

    press_key(&mut rt, Key::Tab);

    assert_eq!(rt.state.closed_by(), None);
}

#[test]
fn a_posted_sound_issue_shows_as_a_warning_status_and_close_still_closes() {
    let mut rt = runtime(shown_long_ago(None, BODY));

    rt.update_if(|state, _| state.set_sound_issue(Some(SOUND_ISSUE.to_owned())));
    settle(&mut rt);
    let tree = rt.accessibility_json();
    let status = a11y_node(&tree, SOUND_ISSUE_KEY).unwrap().clone();
    press(&mut rt, CLOSE_KEY);

    assert_eq!(status["role"], "status", "{status}");
    assert_eq!(
        status["name"],
        format!("Couldn't play the sound: {SOUND_ISSUE}"),
        "{status}"
    );
    assert_eq!(rt.state.closed_by(), Some(ClosedBy::Button));
}

#[test]
fn the_same_sound_answer_twice_changes_nothing() {
    let mut state = shown_long_ago(None, BODY);

    let first = state.set_sound_issue(Some(SOUND_ISSUE.to_owned()));
    let again = state.set_sound_issue(Some(SOUND_ISSUE.to_owned()));

    assert!(first);
    assert!(!again);
    assert_eq!(state.sound_issue(), Some(SOUND_ISSUE));
}

#[test]
fn a_long_body_scrolls_inside_its_viewport_and_close_stays_in_the_window() {
    let long = "The quick brown fox jumps over the lazy dog. ".repeat(120);
    let mut rt = runtime(shown_long_ago(Some(TITLE), &long));
    let [x, y, width, height] = rect(&rt, BODY_KEY);

    rt.wheel(x + width / 2.0, y + height / 2.0, -200.0);
    settle(&mut rt);

    assert!(rect(&rt, BODY_TEXT_KEY)[3] > height);
    assert!(rect(&rt, BODY_TEXT_KEY)[1] < y);
    assert!(bottom(&rt, BODY_KEY) <= rect(&rt, CLOSE_KEY)[1]);
    assert!(bottom(&rt, CLOSE_KEY) <= ALERT_SIZE.1);
}

#[test]
fn a_short_body_does_not_scroll() {
    let mut rt = runtime(shown_long_ago(Some(TITLE), BODY));
    let [x, y, width, height] = rect(&rt, BODY_KEY);

    rt.wheel(x + width / 2.0, y + height / 2.0, -200.0);
    settle(&mut rt);

    assert_eq!(rect(&rt, BODY_TEXT_KEY)[1], y);
}

#[test]
fn a_long_title_and_a_sound_issue_take_one_line_each_and_leave_the_body_three_lines() {
    let title = "T".repeat(140);
    let long = "The quick brown fox jumps over the lazy dog. ".repeat(120);
    let mut rt = runtime(shown_long_ago(Some(&title), &long));

    rt.update_if(|state, _| state.set_sound_issue(Some("word ".repeat(40))));
    settle(&mut rt);

    assert_eq!(rect(&rt, TITLE_KEY)[3], TYPE_TITLE.line_px());
    assert_eq!(rect(&rt, SOUND_ISSUE_KEY)[3], TYPE_CAPTION.line_px());
    assert!(
        rect(&rt, BODY_KEY)[3] >= 3.0 * TYPE_BODY.line_px(),
        "body viewport: {:?}",
        rect(&rt, BODY_KEY)
    );
    assert!(bottom(&rt, CLOSE_KEY) <= ALERT_SIZE.1);
}

#[test]
fn control_and_bidirectional_characters_in_the_title_and_body_show_as_spaces() {
    let state = not_yet_shown(Some("Done\u{202E}now"), "one\u{7}two\nthree\u{202E}four");
    let rt = runtime(state);

    let tree = rt.accessibility_json();
    let dialog = a11y_node(&tree, ROOT_KEY).unwrap();

    assert_eq!(dialog["name"], "Done now");
    assert_eq!(dialog["description"], "one two\nthree four");
}

#[test]
fn the_appearance_hook_reports_a_change_once() {
    let mut state = not_yet_shown(None, BODY);

    let first = set_appearance(&mut state, false);
    let again = set_appearance(&mut state, false);

    assert!(first);
    assert!(!again);
}

#[test]
fn closing_the_window_from_its_own_control_records_the_window() {
    let mut rt = runtime(shown_long_ago(None, BODY));

    rt.update(on_close);

    assert_eq!(rt.state.closed_by(), Some(ClosedBy::Window));
}
