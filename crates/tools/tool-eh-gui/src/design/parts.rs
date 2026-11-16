//! App parts: the styled pieces the kit lacks. Views compose these and kit components; styling
//! happens only here and in the kit.

use super::tokens::{
    COUNT_BADGE_HEIGHT, EMPTY_GLYPH_SIZE, FLOATING_BLUR, FLOATING_HEIGHT, FOCUS_RING_WIDTH,
    FORM_POPOVER_WIDTH, HAIRLINE, ICON_SIZE, LIST_ROW_HEIGHT, LIVE_DOT_SIZE, PRESS_SCALE,
    RADIUS_BADGE, RADIUS_CONTROL, RADIUS_SELECTION, RADIUS_SHEET, RADIUS_TILE, REVEAL_OFFSET,
    SAMPLE_LINES, SELECTION_INSET, SHEET_TOP, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XL, SPACE_XS,
    SPACE_XXL, TILE_MIN_WIDTH, TILE_STAGGER, TYPE_BODY, TYPE_BODY_NUM, TYPE_CAPTION, TYPE_DISPLAY,
    TYPE_HEADLINE, TYPE_LABEL, TYPE_METRIC, TYPE_MONO, TYPE_TITLE,
};
use super::{Surface, floating_tint, motion, selected_secondary_text, selection_fill, surface};
use rune::{
    Color, El, Icon, KeyEvent, ScrollSpec, SpringPair, TextEdit, Transition, Tx, col, el, icon,
    row, spacer, text, text_input,
};
use rune_kit::{IconButton, IconButtonVariant, Kit, RollingText, ScrollBar};

/// The centered empty state that replaces a content area: glyph, title, one muted line, and at
/// most one action button. Enters once with a fade and a small rise.
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

/// [`empty_state`] with `note` under the body line: a [`live_numbers`] line, or the Ghost
/// "View details" of a body line cut short.
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

/// The connection pill's face: a status dot and a short label, focusable with the kit's ring.
pub fn pill_face(k: &Kit, dot: Color, label: &str) -> El {
    row()
        .gap(SPACE_SM)
        .center_y()
        .px(SPACE_MD)
        .h(28.0)
        .focusable(true)
        .focus_ring(FOCUS_RING_WIDTH, 0.0, k.c.border_focus)
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

/// What a sheet is called and how wide it gets.
pub struct SheetFrame<'a> {
    /// The prefix of its keys: `{key}-dialog`, `{key}-scrim`, `{key}-layer`.
    pub key: &'a str,
    /// The title row's text and the dialog's accessible name.
    pub title: &'a str,
    /// The panel's widest width; a narrower window narrows it.
    pub width: f32,
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
    frame: SheetFrame<'_>,
    close: impl Fn(&mut S, &mut Tx) + Clone + 'static,
    body: El,
    footer: El,
    layer: &mut Vec<El>,
) {
    let SheetFrame { key, title, width } = frame;
    let dismiss = IconButton::new(k.glyph.close)
        .variant(IconButtonVariant::Ghost)
        .label("Close")
        .build(k)
        .key("close")
        .on_press_tx(close.clone());
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
        .max_w(width)
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
    let panel = panel.modal_tx(true, close).modal_click_outside(true);

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

/// The fewest lines the body editor shows.
const BODY_EDITOR_MIN_LINES: f32 = 8.0;

/// How tall a mono editor is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EditorHeight {
    /// At least `min_lines` mono lines tall, growing to the space it is given.
    Fill { min_lines: f32 },
    /// Exactly `lines` mono lines tall, never growing.
    Fixed { lines: f32 },
}

/// What a mono editor shows beyond its text.
#[derive(Clone, Copy, Debug)]
pub struct EditorSpec<'a> {
    /// The shell's key; focus moves there.
    pub key: &'a str,
    /// The accessible name.
    pub label: &'a str,
    /// The faint text an empty editor shows.
    pub placeholder: &'a str,
    /// The accessible description, when there is one.
    pub description: Option<&'a str>,
    pub invalid: bool,
    pub height: EditorHeight,
}

/// A multi-line monospace editor for an event body: the kit field's look, at least
/// [`BODY_EDITOR_MIN_LINES`] tall, growing to the space it is given and scrolling past it. The
/// box never springs as lines are typed. There is no `on_submit`, and `on_key` sees every key
/// first: a plain Return reaches it untyped, so `on_key` inserts the newline. `on_change` runs
/// after every edit the field itself makes (typing, paste, undo), not after `on_key`'s.
///
/// The kit's `TextArea` sets its own UI type with no way to change it, so it cannot be mono.
pub fn body_editor<S: 'static>(
    k: &Kit,
    key: &str,
    edit: &TextEdit,
    invalid: bool,
    access: fn(&mut S) -> &mut TextEdit,
    on_key: impl Fn(&mut S, &KeyEvent, &mut Tx) -> bool + 'static,
    on_change: impl Fn(&mut S, &mut Tx) + 'static,
) -> El {
    let spec = EditorSpec {
        key,
        label: "Event body",
        placeholder: "Event body",
        description: None,
        invalid,
        height: EditorHeight::Fill {
            min_lines: BODY_EDITOR_MIN_LINES,
        },
    };
    mono_editor(k, spec, edit, access, on_key, on_change)
}

/// A multi-line monospace editor for short `key=value` lines: the body editor's look, a fixed
/// `spec.height` that never grows, long lines broken anywhere (values such as ids and URLs have
/// no spaces), and a viewport that scrolls only once there is text, so an empty box never takes
/// the wheel from a scrolling pane around it. `on_key` sees every key first, as in
/// [`body_editor`].
pub fn lines_editor<S: 'static>(
    k: &Kit,
    spec: EditorSpec<'_>,
    edit: &TextEdit,
    access: fn(&mut S) -> &mut TextEdit,
    on_key: impl Fn(&mut S, &KeyEvent, &mut Tx) -> bool + 'static,
) -> El {
    mono_editor(k, spec, edit, access, on_key, |_: &mut S, _: &mut Tx| {})
}

/// The shared build of [`body_editor`] and [`lines_editor`].
fn mono_editor<S: 'static>(
    k: &Kit,
    spec: EditorSpec<'_>,
    edit: &TextEdit,
    access: fn(&mut S) -> &mut TextEdit,
    on_key: impl Fn(&mut S, &KeyEvent, &mut Tx) -> bool + 'static,
    on_change: impl Fn(&mut S, &mut Tx) + 'static,
) -> El {
    let c = &k.c;
    let mut field = text_input(edit, TYPE_MONO)
        .key("input")
        .w_full()
        .color(c.text_base)
        .placeholder(spec.placeholder, c.text_faint)
        .caret(c.text_base, k.selection())
        .label(spec.label)
        .invalid(spec.invalid)
        .on_edit(access)
        .on_input_key(on_key)
        .on_input_change(on_change);
    if let Some(description) = spec.description {
        field = field.description(description);
    }
    let line = TYPE_MONO.line_px();
    // (viewport, whether it scrolls, whether the shell grows to the space it is given)
    let (viewport, scrolls, fills) = match spec.height {
        EditorHeight::Fill { min_lines } => (
            col().grow(1.0).min_h(min_lines * line).child(field),
            true,
            true,
        ),
        EditorHeight::Fixed { lines } => (
            col().h(lines * line).child(field.wrap_anywhere()),
            !edit.text.is_empty(),
            false,
        ),
    };
    let mut viewport = viewport.key("scroll").w_full();
    if scrolls {
        viewport = viewport
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .scroll_bar(k);
    }
    let mut shell = col()
        .key(spec.key)
        .role("textbox")
        .w_full()
        .padding(SPACE_SM)
        .radius(RADIUS_CONTROL)
        .fill(c.bg_base)
        .child(viewport);
    if fills {
        shell = shell.grow(1.0).min_h(0.0);
    }
    if spec.invalid {
        shell = shell.outline(HAIRLINE, -HAIRLINE, c.danger.fg);
    } else if edit.focused {
        shell = shell.outline(HAIRLINE, -HAIRLINE, c.border_focus);
    } else {
        for shadow in k.shadow_layers(k.elevation.input()) {
            shell = shell.box_shadow(shadow);
        }
    }
    shell
}

/// A field's issue under it, in the danger color.
pub fn issue(k: &Kit, line: &str) -> El {
    text(line).font(TYPE_CAPTION).color(k.c.danger.fg).wrap()
}

/// `content` entering and leaving with the field-reveal treatment: a fade and a 6 pt rise on
/// the control spring.
pub fn reveal(content: El) -> El {
    content.transition(
        Transition::opacity()
            .combined(Transition::offset(0.0, REVEAL_OFFSET))
            .springs(SpringPair::new(motion::CONTROL, motion::CONTROL_EXIT)),
    )
}

/// A card holding a sample: a header row (`title` in `caption` and `action` at its end) over a
/// viewport a fixed [`SAMPLE_LINES`] of `mono` tall that scrolls, so the card's height never
/// follows its content.
pub fn sample_block(k: &Kit, key: &str, title: &str, action: El, body: El) -> El {
    let header = row()
        .w_full()
        .center_y()
        .child(text(title).font(TYPE_CAPTION).color(k.c.text_muted))
        .child(spacer())
        .child(action);
    let viewport = col()
        .key("sample-scroll")
        .w_full()
        .h(SAMPLE_LINES * TYPE_MONO.line_px())
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(body)
        .scroll_bar(k);
    col()
        .key(key)
        .role("group")
        .label(title)
        .w_full()
        .gap(SPACE_SM)
        .padding(SPACE_MD)
        .radius(RADIUS_TILE)
        .fill(surface(k, Surface::Raised))
        .border(HAIRLINE, k.c.border_weak)
        .child(header)
        .child(viewport)
}

/// A block of selectable monospace text, for a message body.
pub fn body_text(k: &Kit, body: &str) -> El {
    text(body)
        .font(TYPE_MONO)
        .color(k.c.text_base)
        .wrap_anywhere()
        .selectable(true)
}
