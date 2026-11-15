//! App parts: the styled pieces the kit lacks. Views compose these and kit components; styling
//! happens only here and in the kit.

use super::tokens::{
    COUNT_BADGE_HEIGHT, EMPTY_GLYPH_SIZE, FLOATING_BLUR, FLOATING_HEIGHT, FORM_POPOVER_WIDTH,
    HAIRLINE, ICON_SIZE, LIST_ROW_HEIGHT, LIVE_DOT_SIZE, PRESS_SCALE, RADIUS_BADGE,
    RADIUS_SELECTION, RADIUS_SHEET, RADIUS_TILE, SELECTION_INSET, SHEET_TOP, SHEET_WIDTH, SPACE_LG,
    SPACE_MD, SPACE_SM, SPACE_XL, SPACE_XS, SPACE_XXL, TILE_MIN_WIDTH, TILE_STAGGER, TYPE_BODY,
    TYPE_BODY_NUM, TYPE_CAPTION, TYPE_DISPLAY, TYPE_HEADLINE, TYPE_LABEL, TYPE_METRIC, TYPE_MONO,
    TYPE_TITLE,
};
use super::{Surface, floating_tint, motion, selected_secondary_text, selection_fill, surface};
use rune::{Color, El, Icon, SpringPair, Transition, col, el, icon, row, text};
use rune_kit::{IconButton, IconButtonVariant, Kit, RollingText};

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

/// A form label over a control that draws none of its own, such as a mode picker.
pub fn field_label(k: &Kit, label: &str) -> El {
    text(label).font(TYPE_LABEL).color(k.c.text_base)
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

/// A stat tile: a caption over a metric whose digits roll on change. `order` staggers the
/// tiles' entrance, which plays when the strip first appears.
pub fn stat_tile(k: &Kit, key: &str, label: &str, value: Metric, order: usize) -> El {
    let color = if value.warning {
        k.c.warning.fg
    } else {
        k.c.text_base
    };
    col()
        .key(key)
        .gap(SPACE_XS)
        .px(SPACE_MD)
        .py(SPACE_SM)
        .min_w(TILE_MIN_WIDTH)
        .radius(RADIUS_TILE)
        .fill(surface(k, Surface::Raised))
        .border(HAIRLINE, k.c.border_weak)
        .transition(
            Transition::opacity()
                .combined(Transition::offset(0.0, SPACE_SM))
                .springs(motion::SURFACE_PAIR),
        )
        .enter_delay(order as f32 * TILE_STAGGER)
        .child(text(label).font(TYPE_CAPTION).color(k.c.text_muted))
        .child(
            RollingText::new(format!("{key}-value"), value.text)
                .value(value.number)
                .font(TYPE_METRIC)
                .color(color)
                .spring(motion::CONTROL)
                .build(k),
        )
}

/// A stat tile's value: its text, the number it rolls by, and whether it warns.
pub struct Metric {
    pub text: String,
    pub number: f64,
    pub warning: bool,
}

/// A frosted capsule floating over content, for the new-messages pill.
pub fn floating_pill(k: &Kit, key: &str, label: &str) -> El {
    let mut pill = row()
        .key(key)
        .gap(SPACE_SM)
        .center_y()
        .px(SPACE_LG)
        .h(FLOATING_HEIGHT)
        .radius(FLOATING_HEIGHT / 2.0)
        .backdrop_blur(FLOATING_BLUR)
        .fill(floating_tint(k))
        .border(HAIRLINE, k.c.border_weak)
        .press_scale(PRESS_SCALE)
        .transition(
            Transition::opacity()
                .combined(Transition::scale(0.96))
                .springs(SpringPair::new(motion::CONTROL, motion::CONTROL_EXIT)),
        )
        .child(text(label).font(TYPE_LABEL).color(k.c.text_base));
    for shadow in k.shadow_layers(&k.elevation.overlay) {
        pill = pill.box_shadow(shadow);
    }
    pill
}

/// A frosted bar floating over content in the pill's material, holding `children` in a row:
/// controls and live numbers for work that runs over the content, such as a bulk export.
pub fn floating_bar(k: &Kit, key: &str, children: Vec<El>) -> El {
    let mut bar = row()
        .key(key)
        .gap(SPACE_MD)
        .center_y()
        .px(SPACE_LG)
        .py(SPACE_SM)
        .radius(RADIUS_SHEET)
        .backdrop_blur(FLOATING_BLUR)
        .fill(floating_tint(k))
        .border(HAIRLINE, k.c.border_weak)
        .transition(
            Transition::opacity()
                .combined(Transition::offset(0.0, 8.0))
                .springs(motion::SURFACE_PAIR),
        );
    for child in children {
        bar = bar.child(child);
    }
    for shadow in k.shadow_layers(&k.elevation.overlay) {
        bar = bar.box_shadow(shadow);
    }
    bar
}

/// A titled popover surface [`FORM_POPOVER_WIDTH`] wide for a form, with a close button that
/// calls `close`, keyed `{anchor_key}-content`; focus returns to the element keyed
/// `anchor_key` when it closes. The kit's `Popover` caps its surface at 320 pt, too narrow for
/// a row of pickers and a folder path. Mount it with `rune_kit::popover::anchored`.
pub fn form_popover<S: 'static>(
    k: &Kit,
    anchor_key: &str,
    title: &str,
    body: El,
    close: impl Fn(&mut S) + 'static,
) -> El {
    let header = row()
        .key("header")
        .w_full()
        .gap(SPACE_SM)
        .center_y()
        .padding(SPACE_MD)
        .pb(0.0)
        .child(
            text(title)
                .key("title")
                .font(TYPE_HEADLINE)
                .color(k.c.text_base)
                .grow(1.0),
        )
        .child(
            IconButton::new(k.glyph.close)
                .variant(IconButtonVariant::Ghost)
                .label("Close")
                .on_press(close)
                .build(k)
                .key("close"),
        );
    let content = col()
        .key(format!("{anchor_key}-content"))
        .role("dialog")
        .label(title)
        .focus_return(anchor_key)
        .w(FORM_POPOVER_WIDTH)
        .child(header)
        .child(
            col()
                .key("body")
                .w_full()
                .padding(SPACE_MD)
                .shrink()
                .min_h(0.0)
                .child(body),
        );
    rune_kit::popover::raised(k, content, RADIUS_TILE)
}

/// A small warning-colored count, as on the connection pill when the session log has news.
pub fn warning_count(k: &Kit, count: u32) -> El {
    row()
        .center()
        .h(COUNT_BADGE_HEIGHT)
        .min_w(COUNT_BADGE_HEIGHT)
        .px(SPACE_XS)
        .radius(COUNT_BADGE_HEIGHT / 2.0)
        .fill(k.c.warning.fg)
        .child(
            text(count.to_string())
                .font(TYPE_CAPTION)
                .color(k.c.bg_base),
        )
}

/// A warning badge with a short text, for a message whose body is not plain UTF-8.
pub fn warning_badge(k: &Kit, label: &str) -> El {
    row()
        .px(SPACE_SM)
        .py(SPACE_XS)
        .radius(RADIUS_BADGE)
        .border(HAIRLINE, k.c.warning.fg)
        .child(text(label).font(TYPE_CAPTION).color(k.c.warning.fg))
}

/// A list column header cell, `width` wide or taking the rest.
pub fn column_header(k: &Kit, label: &str, width: Option<f32>) -> El {
    cell(text(label).font(TYPE_CAPTION).color(k.c.text_muted), width)
}

/// A list cell holding a copyable value, `width` wide or taking the rest.
pub fn list_cell(k: &Kit, value: &str, width: Option<f32>, selected: bool) -> El {
    let color = if selected {
        selected_secondary_text(k)
    } else {
        k.c.text_base
    };
    cell(text(value).font(TYPE_MONO).color(color).truncate(), width)
}

fn cell(content: El, width: Option<f32>) -> El {
    let cell = row().center_y().min_w(0.0).px(SPACE_SM).child(content);
    match width {
        Some(width) => cell.w(width),
        None => cell.grow(1.0),
    }
}

/// A list row frame: 28 pt, the selection fill inset from the pane edges when selected.
pub fn list_row(k: &Kit, cells: Vec<El>, selected: bool) -> El {
    let mut inner = row()
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .center_y()
        .radius(RADIUS_SELECTION)
        .children(cells);
    if selected {
        inner = inner.fill(selection_fill(k));
    }
    row()
        .w_full()
        .h(LIST_ROW_HEIGHT)
        .px(SELECTION_INSET)
        .child(inner)
}

/// The hairline under a list's header or a strip.
pub fn hairline(k: &Kit) -> El {
    el().w_full().h(HAIRLINE).fill(k.c.border_weak)
}

/// A label over a selectable monospace value inside a card, for the inspector's metadata.
pub fn metadata_card(k: &Kit, rows: Vec<El>) -> El {
    col()
        .w_full()
        .gap(SPACE_MD)
        .padding(SPACE_MD)
        .radius(RADIUS_TILE)
        .fill(surface(k, Surface::Raised))
        .border(HAIRLINE, k.c.border_weak)
        .children(rows)
}

/// A block of selectable monospace text, for a message body.
pub fn body_text(k: &Kit, body: &str) -> El {
    text(body)
        .font(TYPE_MONO)
        .color(k.c.text_base)
        .wrap_anywhere()
        .selectable(true)
}
