//! App parts: the styled pieces the kit lacks. Views compose these and kit components; styling
//! happens only here and in the kit.

use super::motion;
use super::tokens::{
    EMPTY_GLYPH_SIZE, ICON_SIZE, LIVE_DOT_SIZE, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XS, SPACE_XXL,
    TYPE_BODY, TYPE_CAPTION, TYPE_DISPLAY, TYPE_HEADLINE, TYPE_LABEL, TYPE_MONO,
};
use rune::{Color, El, Icon, Transition, col, el, icon, row, text};
use rune_kit::Kit;

/// The centered empty state that replaces a content area: glyph, title, one muted line, and at
/// most one action. Enters once with a fade and a small rise.
pub fn empty_state(
    k: &Kit,
    key: &str,
    glyph: &'static Icon,
    title: &str,
    body: &str,
    action: Option<El>,
) -> El {
    let mut stack = col()
        .key(key)
        .gap(SPACE_MD)
        .center()
        .max_w(420.0)
        .transition(
            Transition::opacity()
                .combined(Transition::offset(0.0, 8.0))
                .springs(motion::SURFACE_PAIR),
        )
        .child(
            icon(glyph, EMPTY_GLYPH_SIZE)
                .color(k.c.icon_muted)
                .mb(SPACE_XS),
        )
        .child(text(title).font(TYPE_DISPLAY).color(k.c.text_base))
        .child(text(body).font(TYPE_BODY).color(k.c.text_muted).wrap());
    if let Some(action) = action {
        stack = stack.child(action.mt(SPACE_SM));
    }
    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .padding(SPACE_XXL)
        .center()
        .child(stack)
}

/// A status dot: 8 pt, in a state color.
pub fn status_dot(color: Color) -> El {
    el().w(LIVE_DOT_SIZE)
        .h(LIVE_DOT_SIZE)
        .radius(LIVE_DOT_SIZE / 2.0)
        .fill(color)
}

/// The connection pill's face: a status dot and a short label.
pub fn pill_face(k: &Kit, dot: Color, label: &str) -> El {
    row()
        .gap(SPACE_SM)
        .center_y()
        .px(SPACE_MD)
        .h(28.0)
        .child(status_dot(dot))
        .child(text(label).font(TYPE_LABEL).color(k.c.text_base))
}

/// A muted icon at the standard size.
pub fn small_icon(k: &Kit, glyph: &'static Icon) -> El {
    icon(glyph, ICON_SIZE).color(k.c.icon_muted)
}

/// A caption label over a selectable monospace value, for paths and ids.
pub fn detail_row(k: &Kit, label: &str, value: &str) -> El {
    col()
        .gap(SPACE_XS)
        .w_full()
        .child(text(label).font(TYPE_CAPTION).color(k.c.text_muted))
        .child(
            text(value)
                .font(TYPE_MONO)
                .color(k.c.text_base)
                .wrap_anywhere()
                .selectable(true),
        )
}

/// A muted caption line, for form notes.
pub fn caption(k: &Kit, line: &str) -> El {
    text(line).font(TYPE_CAPTION).color(k.c.text_muted).wrap()
}

/// A section header in a form or the sidebar.
pub fn section_header(k: &Kit, title: &str) -> El {
    text(title)
        .font(TYPE_HEADLINE)
        .color(k.c.text_base)
        .mt(SPACE_LG)
}
