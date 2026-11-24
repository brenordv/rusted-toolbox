//! seal-gui's own parts: the read-only output box and the list box.

use common_gui_rune::tokens::{
    CONTROL_HEIGHT, HAIRLINE, RADIUS_CONTROL, SPACE_SM, SPACE_XS, TYPE_BODY, TYPE_MONO,
};
use common_gui_rune::{Surface, surface};
use rune::{El, ScrollSpec, col, text};
use rune_kit::{Kit, ScrollBar};

/// A read-only box for a result keyed `key` and named `label`: selectable `mono` text that
/// wraps anywhere, at least `min_lines` lines tall, growing to the space it is given and
/// scrolling inside once it holds text.
pub fn output_box(k: &Kit, key: &str, label: &str, value: &str, min_lines: f32) -> El {
    let mut viewport = col()
        .key(format!("{key}-scroll"))
        .w_full()
        .grow(1.0)
        .min_h(0.0);
    if !value.is_empty() {
        viewport = viewport
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(
                text(value)
                    .font(TYPE_MONO)
                    .color(k.c.text_base)
                    .wrap_anywhere()
                    .selectable(true),
            )
            .scroll_bar(k);
    }
    col()
        .key(key)
        .role("region")
        .label(label)
        .w_full()
        .grow(1.0)
        .min_h(min_lines * TYPE_MONO.line_px() + 2.0 * SPACE_SM)
        .padding(SPACE_SM)
        .radius(RADIUS_CONTROL)
        .fill(surface(k, Surface::Content))
        .border(HAIRLINE, k.c.border_weak)
        .child(viewport)
}

/// The height of a list box that shows `rows` rows.
pub fn list_height(rows: f32) -> f32 {
    rows * CONTROL_HEIGHT + (rows - 1.0) * SPACE_XS + 2.0 * SPACE_SM
}

/// A list box keyed `key`: a group named `label` holding `rows`, `visible_rows` rows tall and
/// scrolling past them; with no rows, `empty` centred in the box, which keeps its height.
pub fn list_box(
    k: &Kit,
    (key, label): (&str, &str),
    rows: Vec<El>,
    empty: &str,
    visible_rows: f32,
) -> El {
    let body = if rows.is_empty() {
        col()
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .center()
            .child(text(empty).font(TYPE_BODY).color(k.c.text_muted).wrap())
    } else {
        col()
            .key(format!("{key}-scroll"))
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(col().w_full().gap(SPACE_XS).children(rows))
            .scroll_bar(k)
    };
    col()
        .key(key)
        .role("group")
        .label(label)
        .w_full()
        .h(list_height(visible_rows))
        .padding(SPACE_SM)
        .radius(RADIUS_CONTROL)
        .fill(surface(k, Surface::Content))
        .border(HAIRLINE, k.c.border_weak)
        .child(body)
}
