//! App parts: the styled pieces the kit lacks. Views compose these and kit components; styling
//! happens only here and in the kit.

use super::motion;
use super::tokens::{
    EMPTY_GLYPH_SIZE, HAIRLINE, ICON_SIZE, LIVE_DOT_SIZE, RADIUS_SHEET, SHEET_TOP, SHEET_WIDTH,
    SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XL, SPACE_XS, SPACE_XXL, TYPE_BODY, TYPE_BODY_NUM,
    TYPE_CAPTION, TYPE_DISPLAY, TYPE_HEADLINE, TYPE_LABEL, TYPE_MONO, TYPE_TITLE,
};
use rune::{Color, El, Icon, Transition, col, el, icon, row, text};
use rune_kit::{IconButton, IconButtonVariant, Kit};

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
    empty_state_with_note(k, key, glyph, title, body, None, action)
}

/// [`empty_state`] with `note` (a [`live_numbers`] line) under the body line.
pub fn empty_state_with_note(
    k: &Kit,
    key: &str,
    glyph: &'static Icon,
    title: &str,
    body: &str,
    note: Option<El>,
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
    if let Some(note) = note {
        stack = stack.child(note);
    }
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

/// A muted line of numbers that change in place, such as live counts; tabular figures keep it
/// from shifting as digits change.
pub fn live_numbers(k: &Kit, line: &str) -> El {
    text(line).font(TYPE_BODY_NUM).color(k.c.text_muted)
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

/// A modal sheet below the toolbar: a fixed title row, `body` (a scroll view, which takes the
/// height left over), and a fixed `footer` under a hairline. The panel is as tall as its
/// content and never taller than the window, so a long form scrolls while the footer's actions
/// stay on screen. Escape, the close button, and a click outside call `close`; focus stays in
/// the panel while it is open. Pushes the scrim and the sheet onto `layer`.
///
/// The kit's `Dialog` is a fixed 368 pt panel centred in the window whose body cannot shrink,
/// so a form longer than that cannot scroll in it.
pub fn sheet<S: 'static>(
    k: &Kit,
    key: &str,
    title: &str,
    close: impl Fn(&mut S) + Clone + 'static,
    body: El,
    footer: El,
    layer: &mut Vec<El>,
) {
    let dismiss = IconButton::new(k.glyph.close)
        .variant(IconButtonVariant::Ghost)
        .label("Close")
        .build(k)
        .key("close")
        .on_press(close.clone());
    let header = row()
        .key("header")
        .w_full()
        .gap(SPACE_SM)
        .center_y()
        .px(SPACE_XL)
        .pt(SPACE_LG)
        .pb(SPACE_SM)
        .child(text(title).font(TYPE_TITLE).color(k.c.text_base).grow(1.0))
        .child(dismiss);
    let footer = col()
        .key("footer")
        .w_full()
        .child(el().w_full().h(HAIRLINE).fill(k.c.border_weak))
        .child(footer.px(SPACE_XL).py(SPACE_LG));

    let mut panel = col()
        .key(format!("{key}-dialog"))
        .role("dialog")
        .label(title)
        .w_full()
        .max_w(SHEET_WIDTH)
        .shrink()
        .min_h(0.0)
        .radius(RADIUS_SHEET)
        .fill(k.c.layer_01)
        .motion(motion::LAYOUT)
        .transition(
            Transition::opacity()
                .combined(Transition::offset(0.0, SPACE_SM))
                .springs(motion::SURFACE_PAIR),
        )
        .child(header)
        .child(body)
        .child(footer);
    for shadow in k.shadow_layers(&k.elevation.overlay) {
        panel = panel.box_shadow(shadow);
    }
    let panel = panel.modal(true, close).modal_click_outside(true);

    layer.push(
        el().key(format!("{key}-scrim"))
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .fill(k.c.scrim)
            .transition(Transition::opacity().springs(motion::SURFACE_PAIR))
            .a11y_hidden(),
    );
    layer.push(
        col()
            .key(format!("{key}-layer"))
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .pt(SHEET_TOP)
            .pb(SPACE_XL)
            .px(SPACE_LG)
            .items_center()
            .child(panel),
    );
}

/// A section header in a form or the sidebar.
pub fn section_header(k: &Kit, title: &str) -> El {
    text(title)
        .font(TYPE_HEADLINE)
        .color(k.c.text_base)
        .mt(SPACE_LG)
}
