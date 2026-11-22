//! Design tokens every Rune window shares: palette seeds and overrides, spacing, radii, type
//! roles, and the few sizes that are the same in every app. A tool's own sizes (its window,
//! panes, and columns) live in that tool.

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

// Spacing, in logical points on a 4 pt base.
pub const SPACE_XS: f32 = 4.0;
pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 12.0;
pub const SPACE_LG: f32 = 16.0;
pub const SPACE_XL: f32 = 24.0;
pub const SPACE_XXL: f32 = 32.0;

/// Hairlines: one logical point in the kit's weak border color.
pub const HAIRLINE: f32 = 1.0;
/// The focus ring an app part draws, the width the kit's own small controls use.
pub const FOCUS_RING_WIDTH: f32 = 2.0;
/// A regular control's height, the smallest pointer target.
pub const CONTROL_HEIGHT: f32 = 28.0;

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
/// Sheet and dialog titles, inspector header.
pub const TYPE_TITLE: TextStyle = TextStyle::new(15.0, 600.0);
/// App name, sidebar section headers.
pub const TYPE_HEADLINE: TextStyle = TextStyle::new(13.0, 600.0);
/// Form text, descriptions, and prose the user writes or reads.
pub const TYPE_BODY: TextStyle = TextStyle::new(13.0, 400.0);
/// Body text with numbers that change in place: tabular figures keep the line still.
pub const TYPE_BODY_NUM: TextStyle = TextStyle::new(13.0, 400.0).features(TNUM);
/// Form labels, buttons.
pub const TYPE_LABEL: TextStyle = TextStyle::new(12.0, 500.0);
/// Column headers, stat labels, secondary metadata; muted.
pub const TYPE_CAPTION: TextStyle = TextStyle::new(11.0, 500.0);
/// Every copyable value: paths, ids, URLs, timestamps, payloads.
pub const TYPE_MONO: TextStyle = TextStyle::new(12.0, 400.0)
    .family(Family::SystemMono)
    .features(TNUM);
