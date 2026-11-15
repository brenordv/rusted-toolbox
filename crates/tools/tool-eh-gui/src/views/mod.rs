//! The view: a pure function of the state. Handlers dispatch inputs; nothing here does I/O.

mod connect;
mod read;
mod sections;
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

/// Keys no focused control took: `mod+f` focuses the Read list's quick search.
pub fn window_key(_: &mut AppState, key: &KeyEvent, tx: &mut Tx) -> bool {
    let focus_search = Chord::parse("mod+f").is_ok_and(|chord| chord.matches_tx(key, tx));
    if focus_search {
        tx.focus(read::SEARCH_KEY, true);
    }
    focus_search
}

#[cfg(test)]
mod tests;
