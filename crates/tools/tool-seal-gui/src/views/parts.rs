//! seal-gui's own parts: the read-only output box, the list box with its drop-target look, and
//! the status badge.

use common_gui_rune::tokens::{
    CONTROL_HEIGHT, FOCUS_RING_WIDTH, HAIRLINE, RADIUS_BADGE, RADIUS_CONTROL, SPACE_SM, SPACE_XS,
    TYPE_BODY, TYPE_CAPTION, TYPE_MONO,
};
use common_gui_rune::{Surface, drop_fill, surface};
use rune::{Color, El, ScrollSpec, col, row, text};
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

/// How tall a [`list_box`] is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ListHeight {
    /// Exactly this many rows.
    Rows(f32),
    /// The space its column gives it, and never fewer than `min_rows` rows.
    Fill { min_rows: f32 },
}

/// The rows a [`list_box`] holds.
pub enum ListRows {
    /// Every row, built.
    Built(Vec<El>),
    /// `count` rows in `column`, a column that builds only the rows in view
    /// (`El::virtual_rows`).
    Virtual { count: usize, column: El },
}

/// A list box keyed `key`: a group named `label` holding `rows`, as tall as `height` says and
/// scrolling past it; with no rows, `empty` centred in the box, which keeps its height.
pub fn list_box(
    k: &Kit,
    (key, label): (&str, &str),
    rows: ListRows,
    empty: &str,
    height: ListHeight,
) -> El {
    let column = match rows {
        ListRows::Built(rows) if rows.is_empty() => None,
        ListRows::Built(rows) => Some(col().w_full().gap(SPACE_XS).children(rows)),
        ListRows::Virtual { count: 0, .. } => None,
        ListRows::Virtual { column, .. } => Some(column),
    };
    let body = match column {
        None => col()
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .center()
            .child(text(empty).font(TYPE_BODY).color(k.c.text_muted).wrap()),
        Some(column) => col()
            .key(format!("{key}-scroll"))
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(column)
            .scroll_bar(k),
    };
    let list = col()
        .key(key)
        .role("group")
        .label(label)
        .w_full()
        .padding(SPACE_SM)
        .radius(RADIUS_CONTROL)
        .fill(surface(k, Surface::Content))
        .border(HAIRLINE, k.c.border_weak)
        .child(body);
    match height {
        ListHeight::Rows(rows) => list.h(list_height(rows)),
        ListHeight::Fill { min_rows } => list.grow(1.0).min_h(list_height(min_rows)),
    }
}

/// `list` as a file drag hovers it: an accent border at the focus ring's width, drawn inside
/// the box so nothing moves, over the drop fill.
pub fn drop_hovered(k: &Kit, list: El) -> El {
    list.fill(drop_fill(k))
        .border(FOCUS_RING_WIDTH, k.c.border_focus)
}

/// What a badge says with its colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeTone {
    /// Waiting its turn.
    Waiting,
    /// In progress.
    Running,
    /// Finished.
    Finished,
    /// Skipped, or cannot run.
    Attention,
    /// Failed.
    Failed,
}

impl BadgeTone {
    pub const ALL: [Self; 5] = [
        Self::Waiting,
        Self::Running,
        Self::Finished,
        Self::Attention,
        Self::Failed,
    ];

    /// The tone's name in a contrast report.
    pub fn name(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Running => "running",
            Self::Finished => "finished",
            Self::Attention => "attention",
            Self::Failed => "failed",
        }
    }

    /// The badge's fill and label colors: the kit's neutral and accent badges, and its state
    /// colors for the outcomes.
    pub fn colors(self, k: &Kit) -> (Color, Color) {
        match self {
            Self::Waiting => (k.c.layer_02, k.c.text_muted),
            Self::Running => (k.c.bg_accent, k.c.text_contrast),
            Self::Finished => (k.c.success.bg, k.c.success.fg),
            Self::Attention => (k.c.warning.bg, k.c.warning.fg),
            Self::Failed => (k.c.danger.bg, k.c.danger.fg),
        }
    }
}

/// A status badge: `label` in `TYPE_CAPTION` on the tone's fill, `width` wide so the text
/// beside a column of badges aligns. It is hidden from assistive technology; the status it
/// shows belongs in the accessible name of what holds it.
pub fn badge(k: &Kit, label: &str, tone: BadgeTone, width: f32) -> El {
    let (fill, ink) = tone.colors(k);
    row()
        .w(width)
        .child(
            row()
                .px(SPACE_XS)
                .radius(RADIUS_BADGE)
                .fill(fill)
                .center_y()
                .child(text(label).font(TYPE_CAPTION).color(ink)),
        )
        .a11y_hidden()
}
