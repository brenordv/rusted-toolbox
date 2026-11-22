//! Design tokens: the shared palette seeds, spacing, radii, and type roles (from
//! `common-gui-rune`) beside eh-gui's own sizes and floating material. Every color, size,
//! radius, and text style a view uses comes from here or from the kit.

pub use common_gui_rune::tokens::{
    CONTROL_HEIGHT, DARK_ACCENT, DARK_BG, DARK_FG, FOCUS_RING_WIDTH, HAIRLINE, LIGHT_ACCENT,
    LIGHT_BG, LIGHT_FG, LIGHT_MUTED, RADIUS_BADGE, RADIUS_CONTROL, RADIUS_SHEET, RADIUS_TILE,
    SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XL, SPACE_XS, SPACE_XXL, SUBMIT_FILL, SUBMIT_HOVER_ALPHA,
    SUBMIT_LABEL, SUBMIT_PRESSED_ALPHA, SUBMIT_SHEEN_ALPHA, TYPE_BODY, TYPE_BODY_NUM, TYPE_CAPTION,
    TYPE_DISPLAY, TYPE_HEADLINE, TYPE_LABEL, TYPE_METRIC, TYPE_MONO, TYPE_TITLE, rgb,
};

/// The floating material's tint alpha over the content surface in light mode; text stays
/// readable where the blur does nothing.
pub const FLOATING_TINT_ALPHA_LIGHT: f32 = 0.78;
/// The dark tint needs more body: at 0.78 over a white backdrop, light text reads 3.3:1.
pub const FLOATING_TINT_ALPHA_DARK: f32 = 0.88;

// Sizes.
pub const TOOLBAR_HEIGHT: f32 = 52.0;
pub const SIDEBAR_WIDTH: f32 = 264.0;
pub const SIDEBAR_MIN: f32 = 220.0;
pub const SIDEBAR_MAX: f32 = 340.0;
pub const INSPECTOR_WIDTH: f32 = 380.0;
pub const INSPECTOR_MIN: f32 = 320.0;
pub const INSPECTOR_MAX: f32 = 560.0;
pub const ICON_SIZE: f32 = 16.0;
pub const EMPTY_GLYPH_SIZE: f32 = 48.0;
pub const LIVE_DOT_SIZE: f32 = 8.0;
/// The connect sheet's widest width; a narrower window narrows it.
pub const SHEET_WIDTH: f32 = 640.0;
/// A control as wide as the sheet's fields: the sheet less its 24 pt inset on each side.
pub const SHEET_FIELD_WIDTH: f32 = SHEET_WIDTH - 2.0 * SPACE_XL;
/// The error details sheet's widest width, wider than a form sheet for long lines of text.
pub const DETAILS_WIDTH: f32 = 720.0;
/// The sheet's top edge, below the toolbar band with room to spare.
pub const SHEET_TOP: f32 = 72.0;
/// The rows of a kit popover's body: the kit popover's 320 pt cap less its 12 pt inset on each
/// side.
pub const POPOVER_WIDTH: f32 = 296.0;
/// A popover holding a form (pickers, switches, a folder with its Change button).
pub const FORM_POPOVER_WIDTH: f32 = 460.0;
/// Message list rows and its header (rune-kit's table sizes).
pub const LIST_ROW_HEIGHT: f32 = 28.0;
pub const LIST_HEADER_HEIGHT: f32 = 30.0;
/// Message list column widths; the preview takes the rest.
pub const COLUMN_ENQUEUED: f32 = 104.0;
pub const COLUMN_PARTITION: f32 = 64.0;
pub const COLUMN_SEQUENCE: f32 = 88.0;
/// Wide enough for "300.00 KB".
pub const COLUMN_SIZE: f32 = 88.0;
/// A stat tile's narrowest: room for "12,403/s" in the metric type; the strip wraps past it.
pub const TILE_MIN_WIDTH: f32 = 120.0;
/// Inset of a selected row's highlight from the pane edges, and its radius.
pub const SELECTION_INSET: f32 = 4.0;
pub const RADIUS_SELECTION: f32 = 6.0;
/// The selection fill: the accent at this alpha (rune-kit's table values).
pub const SELECTION_ALPHA_DARK: f32 = 0.24;
pub const SELECTION_ALPHA_LIGHT: f32 = 0.20;
/// Floating overlays: backdrop blur and height (a capsule).
pub const FLOATING_BLUR: f32 = 12.0;
pub const FLOATING_HEIGHT: f32 = 32.0;
/// Gap between stat tiles entering on the first connect, in seconds.
pub const TILE_STAGGER: f32 = 0.04;
/// Scroll viewport edge fade width.
pub const EDGE_FADE: f32 = 16.0;
/// The quick-search field's width.
pub const SEARCH_WIDTH: f32 = 280.0;
/// The rules sidebar's mode pickers.
pub const SIDEBAR_FIELD_WIDTH: f32 = 220.0;
/// The Send body's caption row: tall enough for a regular control, so a button appearing in it
/// moves nothing.
pub const CAPTION_ROW_HEIGHT: f32 = CONTROL_HEIGHT;
/// The lines a template sample shows before it scrolls; the block never grows past them.
pub const SAMPLE_LINES: f32 = 6.0;
/// The lines the Send properties editor shows before it scrolls; the box never grows past them.
pub const PROPERTY_LINES: f32 = 4.0;
/// A small count badge's height.
pub const COUNT_BADGE_HEIGHT: f32 = 16.0;
/// The press scale of floating controls.
pub const PRESS_SCALE: f32 = 0.97;
/// How far a revealed field rises as it enters.
pub const REVEAL_OFFSET: f32 = 6.0;
pub const WINDOW_SIZE: (f32, f32) = (1280.0, 800.0);
/// The smallest window: the toolbar's controls fit side by side with the sidebar open.
pub const WINDOW_MIN: (f32, f32) = (960.0, 600.0);
/// The content area's narrowest beside the side panes: what the smallest window leaves beside a
/// default sidebar and the narrowest inspector. The message list's fixed columns fit; the
/// preview column is a stub at this width.
pub const CONTENT_MIN: f32 = WINDOW_MIN.0 - SIDEBAR_WIDTH - INSPECTOR_MIN;
/// The window background beside the content where no pane borders it.
pub const FRAME_INSET: f32 = 8.0;
