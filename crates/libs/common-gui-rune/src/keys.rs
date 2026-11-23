//! Key checks a window's key handlers share.

use rune::{Chord, Key, KeyEvent, Mods, Tx};

/// Whether `key` is the chord `text` spells, such as `mod+enter` or `mod+1`. A `text` that is
/// not a chord matches nothing.
pub fn is_chord(text: &str, key: &KeyEvent, tx: &Tx) -> bool {
    Chord::parse(text).is_ok_and(|chord| chord.matches_tx(key, tx))
}

/// Whether `key` is Return, with or without Shift: what types a line break in a multi-line
/// field. A multi-line field without a submit hands a plain Return to the window's default
/// button, so its key handler types the line break itself.
pub fn is_newline(key: &KeyEvent) -> bool {
    key.key == Key::Enter
        && key.mods
            == Mods {
                shift: key.mods.shift,
                ..Mods::default()
            }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_harness::{command, command_mods, press_key, runtime_at};
    use rstest::rstest;
    use rune::{Cx, El, col};

    #[derive(Default)]
    struct Seen {
        mod_enter: usize,
        mod_two: usize,
        not_a_chord: usize,
    }

    fn view(_: &Seen, _: &mut Cx<Seen>) -> El {
        col().w_full().h_full()
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
        true
    }

    #[test]
    fn a_chord_matches_only_its_own_key_and_modifier() {
        let mut rt = runtime_at(Seen::default(), view, (200.0, 100.0));
        rt.on_key = Some(count);

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
}
