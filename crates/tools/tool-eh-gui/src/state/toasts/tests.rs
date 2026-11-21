use super::*;
use crate::form::ConnectForm;
use crate::state::Input;
use chrono::Utc;
use common_cli::app_logger::FileLogging;
use rune_kit::{ToastStackState, ToastTimer};

fn state() -> AppState {
    AppState::new(None, ConnectForm::default(), None, FileLogging::Off)
}

fn texts(s: &AppState) -> Vec<&str> {
    s.toasts()
        .entries()
        .iter()
        .map(|entry| entry.options.text.as_str())
        .collect()
}

fn ids(s: &AppState) -> Vec<u64> {
    s.toasts().entries().iter().map(|entry| entry.id).collect()
}

fn id_of(s: &AppState, text: &str) -> u64 {
    s.toasts()
        .entries()
        .iter()
        .find(|entry| entry.options.text == text)
        .map(|entry| entry.id)
        .unwrap()
}

fn tick(s: &mut AppState) -> Outcome {
    s.apply(Input::Tick(Utc::now()))
}

fn ticks(s: &mut AppState, count: usize) {
    for _ in 0..count {
        tick(s);
    }
}

fn input(s: &mut AppState, toast: ToastInput) -> Outcome {
    s.apply(Input::Toast(toast))
}

fn open_details(s: &mut AppState, toast: Option<u64>) -> Outcome {
    s.apply(Input::ShowDetails {
        title: "Error details".to_owned(),
        text: "whole text".to_owned(),
        toast,
    })
}

/// A collapsed stack of two danger cards and an info card, oldest first: a, b, c.
fn three() -> AppState {
    let mut s = state();
    s.toast(ToastKind::Danger, "a".to_owned());
    s.toast(ToastKind::Danger, "b".to_owned());
    s.toast(ToastKind::Info, "c".to_owned());
    s
}

#[test]
fn an_info_toast_runs_six_clock_seconds_and_a_danger_toast_stays() {
    let mut s = state();

    s.toast(ToastKind::Info, "done".to_owned());
    s.toast(ToastKind::Danger, "broken".to_owned());

    let timers: Vec<ToastTimer> = s.toasts().entries().iter().map(|e| e.timer).collect();
    assert_eq!(
        timers,
        [
            ToastTimer::Running {
                due: INFO_TOAST_SECS
            },
            ToastTimer::Forever
        ]
    );
    assert!(ids(&s)[0] < ids(&s)[1]);
}

#[test]
fn a_toast_past_the_cap_dismisses_the_oldest() {
    let mut s = three();

    s.toast(ToastKind::Danger, "d".to_owned());

    assert_eq!(texts(&s), ["b", "c", "d"]);
}

#[test]
fn a_repeat_of_the_front_card_rings_it_again_each_time() {
    let mut s = state();
    s.toast(ToastKind::Danger, "broken".to_owned());

    s.toast(ToastKind::Danger, "broken".to_owned());
    let first = s.toasts().front().unwrap().options.pulse;
    s.toast(ToastKind::Danger, "broken".to_owned());
    let second = s.toasts().front().unwrap().options.pulse;

    assert_eq!(texts(&s), ["broken"]);
    assert!(first.is_some());
    assert!(second > first);
}

#[test]
fn a_repeated_info_toast_closes_six_ticks_after_the_repeat() {
    let mut s = state();
    s.toast(ToastKind::Info, "done".to_owned());
    ticks(&mut s, 3);

    s.toast(ToastKind::Info, "done".to_owned());
    ticks(&mut s, 5);
    let after_five = texts(&s).len();
    tick(&mut s);

    assert_eq!(after_five, 1);
    assert!(s.toasts().entries().is_empty());
}

#[test]
fn a_buried_copy_comes_to_the_front_under_a_new_id() {
    let mut s = three();
    let old = id_of(&s, "a");

    s.toast(ToastKind::Danger, "a".to_owned());

    assert_eq!(texts(&s), ["b", "c", "a"]);
    assert!(id_of(&s, "a") > old);
}

#[test]
fn the_same_text_as_info_and_danger_stays_two_cards() {
    let mut s = state();

    s.toast(ToastKind::Info, "same".to_owned());
    s.toast(ToastKind::Danger, "same".to_owned());

    assert_eq!(texts(&s), ["same", "same"]);
}

#[test]
fn a_titled_toast_keeps_its_title_and_stays_apart_from_an_untitled_copy() {
    let mut s = state();

    s.toast_titled(ToastKind::Info, Some("Profile switched"), "same".to_owned());
    s.toast(ToastKind::Info, "same".to_owned());

    let titles: Vec<Option<&str>> = s
        .toasts()
        .entries()
        .iter()
        .map(|entry| entry.options.title)
        .collect();
    assert_eq!(titles, [Some("Profile switched"), None]);
}

#[test]
fn a_titled_toast_held_by_the_details_keeps_its_title() {
    let mut s = state();
    s.toast(ToastKind::Danger, "broken".to_owned());
    let held = id_of(&s, "broken");
    open_details(&mut s, Some(held));
    s.toast_titled(
        ToastKind::Info,
        Some("Profile not switched"),
        "later".to_owned(),
    );

    s.apply(Input::CloseDetails);

    let later = s
        .toasts()
        .entries()
        .iter()
        .find(|entry| entry.options.text == "later")
        .map(|entry| entry.options.title);
    assert_eq!(later, Some(Some("Profile not switched")));
}

#[test]
fn an_info_toast_closes_on_its_sixth_tick() {
    let mut s = state();
    s.toast(ToastKind::Info, "done".to_owned());

    ticks(&mut s, 5);
    let after_five = texts(&s).len();
    let sixth = tick(&mut s);

    assert_eq!(after_five, 1);
    assert!(s.toasts().entries().is_empty());
    assert!(sixth.changed);
}

#[test]
fn a_fanned_out_stack_stops_the_countdown_until_it_folds() {
    let mut s = state();
    s.toast(ToastKind::Info, "done".to_owned());
    ticks(&mut s, 2);

    input(&mut s, ToastInput::Pointer(true));
    ticks(&mut s, 10);
    let while_fanned = texts(&s).len();
    input(&mut s, ToastInput::Pointer(false));
    ticks(&mut s, 3);
    let three_after = texts(&s).len();
    tick(&mut s);

    assert_eq!((while_fanned, three_after), (1, 1));
    assert!(s.toasts().entries().is_empty());
}

#[test]
fn a_toast_shown_while_fanned_out_starts_paused() {
    let mut s = state();
    s.toast(ToastKind::Danger, "broken".to_owned());
    input(&mut s, ToastInput::Pointer(true));

    s.toast(ToastKind::Info, "done".to_owned());

    assert_eq!(
        s.toasts().front().unwrap().timer,
        ToastTimer::Paused {
            left: INFO_TOAST_SECS
        }
    );
}

#[test]
fn an_info_toast_keeps_counting_with_the_window_closed() {
    let mut s = state();
    s.toast(ToastKind::Info, "done".to_owned());

    s.apply(Input::WindowClosed(Utc::now()));
    ticks(&mut s, 6);

    assert!(s.toasts().entries().is_empty());
}

#[rstest::rstest]
#[case::pointer(ToastInput::Pointer(true))]
#[case::hotkey(ToastInput::Hotkey)]
fn the_stack_fans_out_and_folds_again(#[case] open: ToastInput) {
    let mut s = three();

    let opened = input(&mut s, open);
    let fanned = s.toasts().stack();
    let folded = input(&mut s, ToastInput::Escape);

    assert!(opened.changed && folded.changed);
    assert_eq!(fanned, ToastStackState::Expanded);
    assert_eq!(s.toasts().stack(), ToastStackState::Collapsed);
}

#[test]
fn the_pointer_leaving_folds_the_stack_and_inputs_that_change_nothing_say_so() {
    let mut s = three();
    input(&mut s, ToastInput::Pointer(true));

    let again = input(&mut s, ToastInput::Pointer(true));
    let left = input(&mut s, ToastInput::Pointer(false));
    let left_again = input(&mut s, ToastInput::Pointer(false));
    let escape_folded = input(&mut s, ToastInput::Escape);

    assert!(!again.changed);
    assert!(left.changed);
    assert!(!left_again.changed);
    assert!(!escape_folded.changed);
    assert_eq!(s.toasts().stack(), ToastStackState::Collapsed);
}

#[rstest::rstest]
#[case::pointer(ToastInput::Pointer(true))]
#[case::hotkey(ToastInput::Hotkey)]
fn an_empty_stack_does_not_fan_out_and_the_next_toast_runs(#[case] open: ToastInput) {
    let mut s = state();

    let out = input(&mut s, open);
    s.toast(ToastKind::Info, "done".to_owned());

    assert!(!out.changed);
    assert_eq!(s.toasts().stack(), ToastStackState::Collapsed);
    assert!(matches!(
        s.toasts().front().unwrap().timer,
        ToastTimer::Running { .. }
    ));
}

#[rstest::rstest]
#[case::pointer(ToastInput::Pointer(true))]
#[case::hotkey(ToastInput::Hotkey)]
fn a_closed_window_does_not_fan_out(#[case] open: ToastInput) {
    let mut s = three();
    s.apply(Input::WindowClosed(Utc::now()));

    let out = input(&mut s, open);

    assert!(!out.changed);
    assert_eq!(s.toasts().stack(), ToastStackState::Collapsed);
}

#[test]
fn the_hotkey_waits_for_the_details_but_not_the_connect_sheet() {
    let mut s = three();
    open_details(&mut s, None);

    let under_details = input(&mut s, ToastInput::Hotkey);
    s.apply(Input::CloseDetails);
    s.apply(Input::OpenSheet(crate::state::SheetPurpose::Connect));
    let under_sheet = input(&mut s, ToastInput::Hotkey);

    assert!(!under_details.changed);
    assert!(under_sheet.changed);
    assert_eq!(s.toasts().stack(), ToastStackState::Expanded);
}

#[test]
fn the_pointer_leaving_under_the_details_keeps_the_stack_fanned_out_until_they_close() {
    let mut s = three();
    input(&mut s, ToastInput::Pointer(true));
    open_details(&mut s, None);

    let under = input(&mut s, ToastInput::Pointer(false));
    s.apply(Input::CloseDetails);
    let after = input(&mut s, ToastInput::Pointer(false));

    assert!(!under.changed);
    assert!(after.changed);
    assert_eq!(s.toasts().stack(), ToastStackState::Collapsed);
}

#[test]
fn the_hold_keeps_the_card_and_queues_what_arrives_until_the_details_close() {
    let mut s = three();
    let a = id_of(&s, "a");
    let before = ids(&s);
    open_details(&mut s, Some(a));

    for text in ["x", "y", "z"] {
        s.apply(Input::ConfigSaved(Err(text.to_owned())));
    }
    s.toast(ToastKind::Danger, "w".to_owned());
    let held = (ids(&s), s.held_toasts());
    let clock = s.toast_clock();
    let held_ticks: Vec<bool> = (0..10).map(|_| tick(&mut s).changed).collect();
    let dismiss = input(&mut s, ToastInput::Dismiss(a));
    let closed = s.apply(Input::CloseDetails);

    assert_eq!(held, (before.clone(), 4));
    assert_eq!(texts(&s).len(), 3);
    assert!(held_ticks.iter().all(|changed| !changed));
    assert_eq!(clock, s.toast_clock());
    assert!(!dismiss.changed);
    assert_eq!(
        texts(&s),
        [
            "Could not save config.json: y",
            "Could not save config.json: z",
            "w"
        ]
    );
    assert_eq!(s.held_toasts(), 0);
    assert_eq!(closed.motion, Motion::Animated);
}

#[test]
fn a_repeat_held_for_a_buried_card_comes_in_at_the_front_and_keeps_the_opener() {
    let mut s = three();
    let (a, b) = (id_of(&s, "a"), id_of(&s, "b"));
    open_details(&mut s, Some(a));

    s.toast(ToastKind::Danger, "b".to_owned());
    let held = texts(&s).join(",");
    s.apply(Input::CloseDetails);

    assert_eq!(held, "a,b,c");
    assert_eq!(texts(&s), ["a", "c", "b"]);
    assert_eq!(id_of(&s, "a"), a);
    assert!(id_of(&s, "b") > b);
}

#[test]
fn details_opened_elsewhere_hold_nothing() {
    let mut s = state();
    s.toast(ToastKind::Info, "done".to_owned());
    let done = id_of(&s, "done");
    open_details(&mut s, None);

    s.toast(ToastKind::Danger, "broken".to_owned());
    let joined = texts(&s).len();
    let dismiss = input(&mut s, ToastInput::Dismiss(done));
    ticks(&mut s, 6);

    assert_eq!(joined, 2);
    assert!(!dismiss.changed);
    assert_eq!(texts(&s), ["broken"]);
    assert_eq!(s.toast_hold(), None);
}

#[test]
fn details_naming_a_card_that_left_hold_nothing() {
    let mut s = state();

    open_details(&mut s, Some(42));
    s.toast(ToastKind::Danger, "broken".to_owned());

    assert_eq!(s.toast_hold(), None);
    assert_eq!(texts(&s), ["broken"]);
}

#[test]
fn a_window_closed_under_the_hold_lets_the_queue_in_and_folds_the_stack() {
    let mut s = three();
    let a = id_of(&s, "a");
    input(&mut s, ToastInput::Pointer(true));
    open_details(&mut s, Some(a));
    s.toast(ToastKind::Danger, "d".to_owned());

    s.apply(Input::WindowClosed(Utc::now()));

    assert_eq!(texts(&s), ["b", "c", "d"]);
    assert_eq!(s.held_toasts(), 0);
    assert_eq!(s.toasts().stack(), ToastStackState::Collapsed);
}

#[rstest::rstest]
#[case::another_card(Some(2))]
#[case::a_card_that_left(Some(42))]
#[case::no_card(None)]
fn details_shown_over_open_ones_keep_the_first_cards_hold(#[case] toast: Option<u64>) {
    let mut s = three();
    let a = id_of(&s, "a");
    open_details(&mut s, Some(a));
    s.toast(ToastKind::Danger, "d".to_owned());

    open_details(&mut s, toast);
    let held = (s.toast_hold(), s.held_toasts());
    s.apply(Input::CloseDetails);

    assert_eq!(held, (Some(a), 1));
    assert_eq!(texts(&s), ["b", "c", "d"]);
}

#[test]
fn a_dismissed_toast_leaves_and_an_unknown_one_changes_nothing() {
    let mut s = three();
    let b = id_of(&s, "b");

    let gone = input(&mut s, ToastInput::Dismiss(b));
    let unknown = input(&mut s, ToastInput::Dismiss(b));

    assert!(gone.changed);
    assert!(!unknown.changed);
    assert_eq!(texts(&s), ["a", "c"]);
}

#[test]
fn dismissing_down_to_one_card_folds_a_fanned_out_stack() {
    let mut s = state();
    s.toast(ToastKind::Danger, "a".to_owned());
    s.toast(ToastKind::Danger, "b".to_owned());
    input(&mut s, ToastInput::Pointer(true));

    let a = id_of(&s, "a");
    input(&mut s, ToastInput::Dismiss(a));

    assert_eq!(s.toasts().stack(), ToastStackState::Collapsed);
}

#[test]
fn a_closed_window_folds_a_fanned_out_stack() {
    let mut s = three();
    input(&mut s, ToastInput::Pointer(true));

    s.apply(Input::WindowClosed(Utc::now()));

    assert_eq!(s.toasts().stack(), ToastStackState::Collapsed);
}

#[test]
fn a_background_toast_and_a_repeat_animate_their_commit() {
    let mut s = state();

    let first = s.apply(Input::ConfigSaved(Err("disk full".to_owned())));
    let repeat = s.apply(Input::ConfigSaved(Err("disk full".to_owned())));

    assert_eq!(first.motion, Motion::Animated);
    assert_eq!(repeat.motion, Motion::Animated);
    assert_eq!(texts(&s).len(), 1);
}

#[rstest::rstest]
#[case::still(Motion::Still, Motion::Animated)]
#[case::animated(Motion::Animated, Motion::Animated)]
#[case::instant(Motion::Instant, Motion::Instant)]
fn a_toast_animates_a_commit_unless_it_lands_at_once(
    #[case] asked: Motion,
    #[case] expected: Motion,
) {
    assert_eq!(toast_motion(asked), expected);
}

#[test]
fn a_tick_that_closes_nothing_changes_nothing() {
    let mut s = state();
    s.toast(ToastKind::Danger, "broken".to_owned());

    let out = tick(&mut s);

    assert!(!out.changed);
}
