//! Design tokens: palette seeds and overrides, spacing, sizes, radii, and type roles. Every
//! color, size, radius, and text style a view uses comes from here or from the kit.

use rune::color::srgb_decode;
use rune::{Color, Family, TextStyle};

/// An opaque sRGB color from `0xRRGGBB`.
pub fn rgb(hex: u32) -> Color {
    let channel = |shift: u32| srgb_decode(((hex >> shift) & 0xFF) as f32 / 255.0);
    Color {
        r: channel(16),
        g: channel(8),
        b: channel(0),
        a: 1.0,
    }
}

// Palette seeds. Neutrals carry a faint cool bias; the accent is an azure-leaning blue.
pub const DARK_BG: u32 = 0x16_16_18;
pub const DARK_FG: u32 = 0xF2_F2_F4;
pub const DARK_ACCENT: u32 = 0x3D_8B_FF;
pub const LIGHT_BG: u32 = 0xFF_FF_FF;
pub const LIGHT_FG: u32 = 0x1A_1A_1C;
pub const LIGHT_ACCENT: u32 = 0x0A_66_E0;

/// The `Submit` fill in both appearances, darker than either accent: a white label on the
/// bright dark accent fails (3.3:1), and on `#0A66E0` it drops under 4.5:1 once the sheen
/// composites. White on this fill is 7.4:1 at rest and 4.9:1 under the sheen.
pub const SUBMIT_FILL: u32 = 0x00_50_B8;
/// The `Submit` label in both appearances.
pub const SUBMIT_LABEL: u32 = 0xFF_FF_FF;
/// The kit's top sheen on `Submit` (white at 0.20) drops the label under 4.5:1; 0.08 keeps it.
pub const SUBMIT_SHEEN_ALPHA: f32 = 0.08;
/// `Submit` hover darkens in both appearances; the light kit's white hover overlay would
/// brighten the fill under a white label (3.5:1).
pub const SUBMIT_HOVER_ALPHA: f32 = 0.12;
/// `Submit` press darkens too, for the same reason.
pub const SUBMIT_PRESSED_ALPHA: f32 = 0.20;
/// Light muted text. The kit floors muted text against white only, which lands near 4.4:1 on
/// the light window surface.
pub const LIGHT_MUTED: u32 = 0x6E_6E_73;
/// The floating material's tint alpha over the content surface in light mode; text stays
/// readable where the blur does nothing.
pub const FLOATING_TINT_ALPHA_LIGHT: f32 = 0.78;
/// The dark tint needs more body: at 0.78 over a white backdrop, light text reads 3.3:1.
pub const FLOATING_TINT_ALPHA_DARK: f32 = 0.88;

// Spacing, in logical points on a 4 pt base.
pub const SPACE_XS: f32 = 4.0;
pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 12.0;
pub const SPACE_LG: f32 = 16.0;
pub const SPACE_XL: f32 = 24.0;
pub const SPACE_XXL: f32 = 32.0;

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
/// Hairlines: one logical point in the kit's weak border color.
pub const HAIRLINE: f32 = 1.0;
/// The focus ring an app part draws, the width the kit's own small controls use.
pub const FOCUS_RING_WIDTH: f32 = 2.0;
pub const SHEET_WIDTH: f32 = 520.0;
/// The error details sheet's widest width, wider than a form sheet for long lines of text.
pub const DETAILS_WIDTH: f32 = 720.0;
/// The sheet's top edge, below the toolbar band with room to spare.
pub const SHEET_TOP: f32 = 72.0;
pub const POPOVER_WIDTH: f32 = 360.0;
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
pub const CAPTION_ROW_HEIGHT: f32 = 28.0;
/// The lines a template sample shows before it scrolls; the block never grows past them.
pub const SAMPLE_LINES: f32 = 6.0;
/// A small count badge's height.
pub const COUNT_BADGE_HEIGHT: f32 = 16.0;
/// The press scale of floating controls.
pub const PRESS_SCALE: f32 = 0.97;
/// How far a revealed field rises as it enters.
pub const REVEAL_OFFSET: f32 = 6.0;
pub const WINDOW_SIZE: (f32, f32) = (1280.0, 800.0);
/// The smallest window: the toolbar's controls fit side by side with the sidebar open.
pub const WINDOW_MIN: (f32, f32) = (960.0, 600.0);

// Radius by role.
pub const RADIUS_BADGE: f32 = 4.0;
pub const RADIUS_CONTROL: f32 = 7.0;
pub const RADIUS_TILE: f32 = 10.0;
pub const RADIUS_SHEET: f32 = 14.0;

// Type roles. Weight carries emphasis before size; no text role outside this list.
const TNUM: &str = "'tnum' 1";
/// Empty-state titles only.
pub const TYPE_DISPLAY: TextStyle = TextStyle::new(20.0, 600.0);
/// Stat values.
pub const TYPE_METRIC: TextStyle = TextStyle::new(22.0, 600.0).features(TNUM);
/// Sheet titles, inspector header.
pub const TYPE_TITLE: TextStyle = TextStyle::new(15.0, 600.0);
/// App name, sidebar section headers.
pub const TYPE_HEADLINE: TextStyle = TextStyle::new(13.0, 600.0);
/// Form text, descriptions.
pub const TYPE_BODY: TextStyle = TextStyle::new(13.0, 400.0);
/// Body text with numbers that change in place: tabular figures keep the line still.
pub const TYPE_BODY_NUM: TextStyle = TextStyle::new(13.0, 400.0).features(TNUM);
/// Form labels, buttons.
pub const TYPE_LABEL: TextStyle = TextStyle::new(12.0, 500.0);
/// Column headers, stat labels, secondary metadata; muted.
pub const TYPE_CAPTION: TextStyle = TextStyle::new(11.0, 500.0);
/// Every copyable value: paths, ids, timestamps, bodies.
pub const TYPE_MONO: TextStyle = TextStyle::new(12.0, 400.0)
    .family(Family::SystemMono)
    .features(TNUM);
