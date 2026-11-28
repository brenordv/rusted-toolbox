//! seal-gui's own parts: the read-only output box and the list box's drop-target look.

use common_gui_rune::tokens::{FOCUS_RING_WIDTH, HAIRLINE, RADIUS_CONTROL, SPACE_SM, TYPE_MONO};
use common_gui_rune::{Surface, drop_fill, surface};
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

/// `list` as a file drag hovers it: an accent border at the focus ring's width, drawn inside
/// the box so nothing moves, over the drop fill.
pub fn drop_hovered(k: &Kit, list: El) -> El {
    list.fill(drop_fill(k))
        .border(FOCUS_RING_WIDTH, k.c.border_focus)
}
