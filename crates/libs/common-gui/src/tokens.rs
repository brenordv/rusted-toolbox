use egui::{Color32, FontFamily, FontId};

// ---- Spacing (4px base scale) ----

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

// ---- Typography (egui's bundled fonts; emphasis via size and color, not weight) ----

/// The four type roles every piece of text maps onto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// The [`FontId`] for a type role. All sizes are logical points; egui applies
/// the display's pixels-per-point itself.
pub fn get_font(ty: Typography) -> FontId {
    match ty {
        Typography::TextBody => FontId::new(13.0, FontFamily::Proportional),
        Typography::TextSmall => FontId::new(11.0, FontFamily::Proportional),
        Typography::TextHeading => FontId::new(15.0, FontFamily::Proportional),
        Typography::TextMono => FontId::new(12.5, FontFamily::Monospace),
    }
}

pub fn text_body() -> FontId {
    get_font(Typography::TextBody)
}

pub fn text_small() -> FontId {
    get_font(Typography::TextSmall)
}

pub fn text_heading() -> FontId {
    get_font(Typography::TextHeading)
}

pub fn text_mono() -> FontId {
    get_font(Typography::TextMono)
}

// ---- Palette (dark; one accent, three status colors) ----

/// The fixed color palette. Both GUI consumers ship [`Palette::default`]
/// as-is; per-app knobs only get added when a second app actually needs to
/// differ.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_scale_strictly_ascends() {
        let scale = [SPACE_XS, SPACE_SM, SPACE_MD, SPACE_LG, SPACE_XL];

        assert!(scale.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn font_tokens_carry_the_documented_sizes_and_families() {
        let cases = [
            (text_body(), 13.0, FontFamily::Proportional),
            (text_small(), 11.0, FontFamily::Proportional),
            (text_heading(), 15.0, FontFamily::Proportional),
            (text_mono(), 12.5, FontFamily::Monospace),
        ];

        for (font, size, family) in cases {
            assert_eq!(font.size, size);
            assert_eq!(font.family, family);
        }
    }

    #[test]
    fn wrapper_fns_match_get_font() {
        assert_eq!(text_body().size, get_font(Typography::TextBody).size);
        assert_eq!(text_small().size, get_font(Typography::TextSmall).size);
        assert_eq!(text_heading().size, get_font(Typography::TextHeading).size);
        assert_eq!(text_mono().size, get_font(Typography::TextMono).size);
    }

    fn relative_luminance(color: Color32) -> f32 {
        fn linear(channel: u8) -> f32 {
            let c = f32::from(channel) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }

        0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
    }

    #[test]
    fn text_colors_clear_every_background_by_a_readable_margin() {
        let palette = Palette::default();
        let backgrounds = [palette.bg_base, palette.bg_panel, palette.bg_raised];

        for text in [palette.text_primary, palette.text_muted] {
            for bg in backgrounds {
                let delta = relative_luminance(text) - relative_luminance(bg);
                assert!(delta >= 0.25, "luminance delta {delta} is below 0.25");
            }
        }
    }
}
