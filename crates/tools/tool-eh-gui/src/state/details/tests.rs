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

#[rstest::rstest]
#[case::empty(0)]
#[case::one_short(139)]
#[case::at_the_cap(140)]
fn a_text_up_to_the_cap_shows_whole(#[case] chars: usize) {
    let text = "a".repeat(chars);

    let preview = error_preview(&text);

    assert_eq!(
        preview,
        ErrorPreview {
            shown: text,
            cut: false
        }
    );
}

#[test]
fn a_text_past_the_cap_shows_its_first_characters_and_an_ellipsis() {
    let text = format!("{}b", "a".repeat(ERROR_PREVIEW_CHARS));

    let preview = error_preview(&text);

    assert!(preview.cut);
    assert_eq!(
        preview.shown,
        format!("{}…", "a".repeat(ERROR_PREVIEW_CHARS))
    );
}

#[test]
fn a_cut_never_splits_a_character() {
    let text = "é".repeat(200);

    let preview = error_preview(&text);

    assert_eq!(
        preview.shown,
        format!("{}…", "é".repeat(ERROR_PREVIEW_CHARS))
    );
}

#[test]
fn line_breaks_and_unsafe_characters_become_spaces() {
    let preview = error_preview("one\ntwo\tthree\u{2028}four\u{202E}five");

    assert_eq!(preview.shown, "one two three four five");
    assert!(!preview.cut);
}

#[test]
fn a_cut_drops_the_spaces_before_the_ellipsis() {
    let text = format!("{}   tail", "a".repeat(ERROR_PREVIEW_CHARS - 2));

    let preview = error_preview(&text);

    assert_eq!(
        preview.shown,
        format!("{}…", "a".repeat(ERROR_PREVIEW_CHARS - 2))
    );
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
