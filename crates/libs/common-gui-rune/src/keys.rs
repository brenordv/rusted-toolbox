//! Key checks a window's key handlers share.

use rune::{Chord, Key, KeyEvent, Mods, TextEdit, Tx};

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

/// The area `mod+1` through `mod+<count>` picks, counting from 0, as a fresh press: a key the
/// OS repeats while it is held picks nothing, so holding the chord does not flick between
/// areas.
pub fn area_chord(count: usize, key: &KeyEvent, tx: &Tx) -> Option<usize> {
    if tx.key_repeat() {
        return None;
    }
    (0..count).find(|index| is_chord(&format!("mod+{}", index + 1), key, tx))
}

/// Types a line break into `edit` at the caret, over any selection, when `key` is Return with
/// or without Shift ([`is_newline`]); true when it did. The field's change hook does not run for
/// this insert.
pub fn type_newline(edit: &mut TextEdit, key: &KeyEvent) -> bool {
    if !is_newline(key) {
        return false;
    }
    edit.insert("\n");
    true
}

#[cfg(test)]
mod tests;
