//! The view: a pure function of the state. Handlers dispatch inputs; nothing here does I/O.

mod connect;
mod sections;
mod shell;

use crate::state::AppState;
use rune::{Cx, El};

/// The window's root.
pub fn view(s: &AppState, cx: &mut Cx<AppState>) -> El {
    let k = s.ds.kit(s.system_dark.unwrap_or(cx.system_dark()));
    let mut layer = Vec::new();
    let body = shell::shell(s, cx, k, &mut layer);
    connect::sheet(s, k, &mut layer);
    shell::toasts(s, k, &mut layer);
    rune_kit::root(body, layer)
}

#[cfg(test)]
mod tests;
