use super::*;
use crate::form::ConnectForm;
use crate::state::Input;
use chrono::TimeDelta;
use common_cli::app_logger::FileLogging;

fn state() -> AppState {
    AppState::new(None, ConnectForm::default(), None, FileLogging::Off)
}

fn open(s: &mut AppState, text: &str) {
    s.apply(Input::ShowDetails {
        title: "Couldn't connect".to_owned(),
        text: text.to_owned(),
        toast: None,
    });
}

#[test]
fn opening_keeps_line_breaks_maps_each_line_and_closes_the_pill_popover() {
    let mut s = state();
    s.pill_open = true;

    open(&mut s, "first\u{202E} line\r\nsecond\tline");

    let details = s.details.as_ref().unwrap();
    assert_eq!(details.title, "Couldn't connect");
    assert_eq!(details.text, "first  line\nsecond line");
    assert_eq!(details.copied_until, None);
    assert!(!s.pill_open);
}

#[test]
fn closing_closes_and_closing_again_changes_nothing() {
    let mut s = state();
    open(&mut s, "boom");

    let first = s.apply(Input::CloseDetails);
    let second = s.apply(Input::CloseDetails);

    assert!(s.details.is_none());
    assert!(first.changed);
    assert!(!second.changed);
}

#[test]
fn a_copy_while_closed_changes_nothing() {
    let mut s = state();

    let out = s.apply(Input::DetailsCopied(Utc::now()));

    assert!(!out.changed);
    assert!(s.details.is_none());
}

#[test]
fn copied_shows_until_its_time_is_up() {
    let mut s = state();
    open(&mut s, "boom");
    let at = Utc::now();
    s.apply(Input::DetailsCopied(at));

    let early = s.apply(Input::Tick(at + TimeDelta::milliseconds(1_000)));
    let still = s.details.as_ref().unwrap().copied_until;
    let due = s.apply(Input::Tick(at + FEEDBACK_FOR));

    assert_eq!(still, Some(at + FEEDBACK_FOR));
    assert!(!early.changed);
    assert!(due.changed);
    assert_eq!(s.details.as_ref().unwrap().copied_until, None);
}

#[test]
fn closing_the_window_closes_the_modal() {
    let mut s = state();
    open(&mut s, "boom");

    s.apply(Input::WindowClosed(Utc::now()));

    assert!(s.details.is_none());
}
