use super::*;
use crate::Ds;
use crate::parts::{EditorHeight, EditorSpec, text_editor};
use crate::test_harness::{command, command_mods, press, press_key, runtime_with_keys, settle};
use crate::tokens::TYPE_BODY;
use rstest::rstest;
use rune::{Cx, El, Runtime, col};

const EDITOR_KEY: &str = "notes";

#[derive(Default)]
struct Seen {
    mod_enter: usize,
    mod_two: usize,
    not_a_chord: usize,
    areas: Vec<usize>,
    edit: TextEdit,
    ds: Ds,
}

fn view(seen: &Seen, _: &mut Cx<Seen>) -> El {
    let spec = EditorSpec {
        key: EDITOR_KEY,
        label: "Notes",
        placeholder: "",
        description: None,
        invalid: false,
        height: EditorHeight::Fixed { lines: 3.0 },
        font: TYPE_BODY,
    };
    col().w_full().h_full().child(text_editor(
        seen.ds.kit(true),
        spec,
        &seen.edit,
        |seen: &mut Seen| &mut seen.edit,
        |seen: &mut Seen, key: &KeyEvent, _: &mut Tx| type_newline(&mut seen.edit, key),
        |_: &mut Seen, _: &mut Tx| {},
    ))
}

fn count(seen: &mut Seen, key: &KeyEvent, tx: &mut Tx) -> bool {
    if is_chord("mod+enter", key, tx) {
        seen.mod_enter += 1;
    }
    if is_chord("mod+2", key, tx) {
        seen.mod_two += 1;
    }
    if is_chord("no such chord", key, tx) {
        seen.not_a_chord += 1;
    }
    if let Some(area) = area_chord(3, key, tx) {
        seen.areas.push(area);
    }
    true
}

fn runtime() -> Runtime<Seen> {
    runtime_with_keys(Seen::default(), view, count, (240.0, 160.0))
}

#[test]
fn a_chord_matches_only_its_own_key_and_modifier() {
    let mut rt = runtime();

    command(&mut rt, Key::Enter);
    command(&mut rt, Key::Command('2'));
    command(&mut rt, Key::Command('2'));
    press_key(&mut rt, Key::Enter);

    assert_eq!(
        (rt.state.mod_enter, rt.state.mod_two, rt.state.not_a_chord),
        (1, 2, 0)
    );
}

#[rstest]
#[case::plain_return(Key::Enter, Mods::default(), true)]
#[case::shift_return(Key::Enter, Mods { shift: true, ..Mods::default() }, true)]
#[case::mod_return(Key::Enter, command_mods(), false)]
#[case::alt_return(Key::Enter, Mods { alt: true, ..Mods::default() }, false)]
#[case::another_key(Key::Tab, Mods::default(), false)]
fn only_return_with_or_without_shift_types_a_line_break(
    #[case] key: Key,
    #[case] mods: Mods,
    #[case] newline: bool,
) {
    assert_eq!(is_newline(&KeyEvent { key, mods }), newline);
}

#[test]
fn the_area_chords_pick_their_areas_and_nothing_past_the_count() {
    let mut rt = runtime();

    command(&mut rt, Key::Command('1'));
    command(&mut rt, Key::Command('2'));
    command(&mut rt, Key::Command('3'));
    command(&mut rt, Key::Command('4'));
    press_key(&mut rt, Key::Text("2".into()));

    assert_eq!(rt.state.areas, vec![0, 1, 2]);
}

#[test]
fn a_held_area_chord_picks_nothing() {
    let mut rt = runtime();
    command(&mut rt, Key::Command('2'));

    rt.repeat_key(KeyEvent {
        key: Key::Command('2'),
        mods: command_mods(),
    });
    settle(&mut rt);

    assert_eq!(rt.state.areas, vec![1]);
}

#[test]
fn return_types_a_line_break_at_the_caret() {
    let mut seen = Seen {
        edit: TextEdit::new("ab"),
        ..Seen::default()
    };
    seen.edit.set_caret(1);
    let mut rt = runtime_with_keys(seen, view, |_, _, _| false, (240.0, 160.0));
    rt.update_if(|_, tx| {
        tx.focus(EDITOR_KEY, false);
        true
    });
    settle(&mut rt);

    press_key(&mut rt, Key::Enter);

    assert_eq!(rt.state.edit.text, "a\nb");
}

#[test]
fn return_types_a_line_break_over_the_selection() {
    let mut rt = runtime_with_keys(Seen::default(), view, |_, _, _| false, (240.0, 160.0));
    press(&mut rt, EDITOR_KEY);
    rt.paste("one two");
    settle(&mut rt);
    command(&mut rt, Key::Command('a'));

    press_key(&mut rt, Key::Enter);

    assert_eq!(rt.state.edit.text, "\n");
}

#[test]
fn mod_return_types_nothing() {
    let mut edit = TextEdit::new("ab");

    let typed = type_newline(
        &mut edit,
        &KeyEvent {
            key: Key::Enter,
            mods: command_mods(),
        },
    );

    assert!(!typed);
    assert_eq!(edit.text, "ab");
}
