use egui::{Color32, FontFamily, FontId};

/// ----------------------------------------------------------------------------
/// ---- SPACING ----

/// icon-to-label gaps, tight inline runs
pub const SPACE_XS: f32 = 4.0;

/// item_spacing inside groups, button padding x
pub const SPACE_SM: f32 = 8.0;

/// between form rows, section padding
pub const SPACE_MD: f32 = 12.0;

/// panel margins, between sections
pub const SPACE_LG: f32 = 16.0;

/// page margins, tab content inset
pub const SPACE_XL: f32 = 24.0;

/// ----------------------------------------------------------------------------
/// ---- TYPOGRAPHY ----

pub enum Typography {
    /// default, labels, list rows
    TextBody,

    /// secondary info, badges
    TextSmall,

    /// section titles, tab labels
    TextHeading,

    /// keys, armored text, paths, sizes
    TextMono,
}

pub fn get_font(ty: Typography) -> FontId {
    match ty {
        Typography::TextBody => FontId::new(13.0, FontFamily::Proportional),
        Typography::TextSmall => FontId::new(11.0, FontFamily::Proportional),
        Typography::TextHeading => FontId::new(15.0, FontFamily::Proportional),
        Typography::TextMono => FontId::new(12.5, FontFamily::Monospace),
    }
}

pub fn text_body() -> FontId { get_font(Typography::TextBody) }
pub fn text_small() -> FontId { get_font(Typography::TextSmall) }
pub fn text_heading() -> FontId { get_font(Typography::TextHeading) }
pub fn text_mono() -> FontId { get_font(Typography::TextMono) }




/// ----------------------------------------------------------------------------
/// ---- PALETTE ----

#[derive(Debug, Clone)]
pub struct Palette {
    /// window background
    pub bg_base: Color32,

    /// panels, input, table headers
    pub bg_panel: Color32,

    /// hovered rows, raised cards
    pub bg_raised: Color32,

    /// 1px strokes everywhere
    pub border: Color32,

    /// body text
    pub text_primary: Color32,

    /// labels, secondary
    pub text_muted: Color32,

    /// primary buttons, selection, active tab
    pub accent: Color32,

    /// success badges/toasts
    pub ok: Color32,

    /// attention badges/toasts
    pub warning: Color32,

    /// errors, destructive confirms
    pub error: Color32,
}

impl Default for Palette {
    fn default() -> Self {
        Self { 
            bg_base: Color32::from_rgb(0x1E, 0x1F, 0x22),
            bg_panel: Color32::from_rgb(0x2B, 0x2D, 0x30),
            bg_raised: Color32::from_rgb(0x33, 0x35, 0x3A),
            border: Color32::from_rgb(0x3C, 0x3F, 0x44),
            text_primary: Color32::from_rgb(0xDF, 0xE1, 0xE5),
            text_muted: Color32::from_rgb(0x9D, 0xA0, 0xA8),
            accent: Color32::from_rgb(0x35, 0x74, 0xF0),
            ok: Color32::from_rgb(0x4C, 0xAF, 0x50),
            warning: Color32::from_rgb(0xE8, 0xA3, 0x3D),
            error: Color32::from_rgb(0xE5, 0x53, 0x4B),
        }
    }
}
