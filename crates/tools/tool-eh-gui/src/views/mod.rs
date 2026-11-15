//! The view: a pure function of the state. Handlers dispatch inputs; nothing here does I/O.

mod connect;
mod export;
mod inspector;
mod message_list;
mod read;
mod rules_form;
mod sections;
mod send;
mod shell;

use crate::state::AppState;
use rune::{Chord, Cx, El, KeyEvent, Tx};

/// The window's root.
pub fn view(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let dark = s.system_dark.unwrap_or(cx.system_dark());
    let k = s.ds.kit(dark);
    let mut layer = Vec::new();
    let body = shell::shell(s, cx, k, dark, &mut layer);
    connect::sheet(s, k, &mut layer);
    shell::toasts(s, k, &mut layer);
    rune_kit::root(body, layer)
}

/// Keys no focused control took: the Send shortcuts (`mod+enter` while the Send section shows,
/// `mod+1..3` to pick a section), and `mod+f`, which focuses the Read list's quick search.
pub fn window_key(s: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    if send::send_keys(s, key, tx) {
        return true;
    }
    let focus_search = Chord::parse("mod+f").is_ok_and(|chord| chord.matches_tx(key, tx));
    if focus_search {
        tx.focus(read::SEARCH_KEY, true);
    }
    focus_search
}

#[cfg(test)]
mod tests;
