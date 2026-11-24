//! The seal window's own sizes, and the one color role it draws differently from the shared
//! kits. Everything else (colors, spacing, type, motion) comes from `common-gui-rune`.

use common_gui_rune::Ds;
use rune_kit::Kit;

/// The window's size when it opens.
pub const WINDOW_SIZE: (f32, f32) = (980.0, 720.0);
/// The smallest window. The width keeps the form column's controls side by side. The height
/// is the Text tab at rest with nothing to scroll: the header row (60 = 2 x 16 inset + 28),
/// its hairline (1), the form's insets (32), Direction (16 label + 4 + 28 switch), Input
/// (12 + 16 + 4 + 4 lines of mono at 16 + 16 padding), the Recipients group (24 + 16 + 18
/// header + 8 + 3 rows of 32 + 8, then 12 + 16 + 4 + 28 for Other key), Output (24 + 28 + 4
/// + 80), and the action row (1 + 52): 676, under the design system's 680 ceiling.
pub const WINDOW_MIN: (f32, f32) = (760.0, 680.0);
/// The form column's widest width: 64 columns of armored text in `mono` with the box's
/// padding.
pub const FORM_MAX_WIDTH: f32 = 640.0;
/// The fewest lines the input and output boxes show.
pub const TEXT_MIN_LINES: f32 = 4.0;
/// The recipient rows the Text tab's list shows before it scrolls.
pub const TEXT_RECIPIENT_ROWS: f32 = 3.0;
/// The recipient rows the Files tab's list shows before it scrolls; the file list takes the
/// height the Text tab gives its input.
pub const FILES_RECIPIENT_ROWS: f32 = 2.0;
/// The fewest rows the Files list shows.
pub const FILES_MIN_ROWS: f32 = 3.0;
/// The badge column of a Files row: the widest status label in `TYPE_CAPTION` with the badge's
/// padding.
pub const FILE_BADGE_WIDTH: f32 = 56.0;
/// The size column of a Files row: eleven characters ("1,023.99 MB") in `TYPE_MONO`.
pub const FILE_SIZE_WIDTH: f32 = 84.0;

/// The kits seal-gui draws its checkboxes and radio buttons with. The kit draws an empty
/// checkbox's only edge in its weak border color, near 1.4:1 on the content surface, and an
/// empty radio's ring in its strong border color, near 1.2:1 there in light mode; these copies
/// draw both in the muted icon color, over 5:1, and a selected radio's dot in white, where the
/// kit's knob color is 2.96:1 on the dark accent. Every other role, the hairlines' among them,
/// stays as it is.
pub struct ChoiceKits {
    dark: Kit,
    light: Kit,
}

impl ChoiceKits {
    pub fn new() -> Self {
        let ds = Ds::new();
        let edged = |dark: bool| {
            let mut kit = ds.kit(dark).clone();
            kit.c.border_weak = kit.c.icon_muted;
            kit.c.border_strong = kit.c.icon_muted;
            kit.c.knob_on = kit.c.white;
            kit
        };
        Self {
            dark: edged(true),
            light: edged(false),
        }
    }

    /// The kit for checkboxes and radios in the appearance the window draws in.
    pub fn kit(&self, dark: bool) -> &Kit {
        if dark { &self.dark } else { &self.light }
    }
}

impl Default for ChoiceKits {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::BadgeTone;
    use common_gui_rune::contrast::{Contract, NON_TEXT, TEXT, appearance_name, over};
    use common_gui_rune::{Surface, drop_fill, surface};

    #[test]
    fn checkboxes_and_radios_are_marked_at_3_to_1_on_the_content_surface() {
        let kits = ChoiceKits::new();
        let mut contract = Contract::default();

        for dark in [true, false] {
            let k = kits.kit(dark);
            let mode = appearance_name(dark);
            let content = surface(k, Surface::Content);
            let checked_fill = over(k.c.surface_weak, content);
            let radio_fill = over(k.c.bg_base, content);
            let radio_on = over(k.c.bg_accent, content);
            contract.check(
                &format!("{mode} empty checkbox edge"),
                over(k.c.border_weak, content),
                content,
                NON_TEXT,
            );
            contract.check(
                &format!("{mode} check mark"),
                over(k.c.std_icon, checked_fill),
                checked_fill,
                NON_TEXT,
            );
            contract.check(
                &format!("{mode} empty radio ring"),
                over(k.c.border_strong, radio_fill),
                content,
                NON_TEXT,
            );
            contract.check(
                &format!("{mode} selected radio on the list"),
                radio_on,
                content,
                NON_TEXT,
            );
            contract.check(
                &format!("{mode} selected radio dot"),
                over(k.c.knob_on, radio_on),
                radio_on,
                NON_TEXT,
            );
        }

        contract.assert_met();
    }

    #[test]
    fn each_badge_label_meets_the_text_contrast_on_the_list_and_on_the_drop_fill() {
        let ds = Ds::new();
        let mut contract = Contract::default();

        for dark in [true, false] {
            let k = ds.kit(dark);
            let mode = appearance_name(dark);
            let content = surface(k, Surface::Content);
            let hovered = over(drop_fill(k), content);
            for (under_name, under) in [("list", content), ("drop fill", hovered)] {
                for tone in BadgeTone::ALL {
                    let (fill, ink) = tone.colors(k);
                    let fill = over(fill, under);
                    contract.check(
                        &format!("{mode} {} badge on the {under_name}", tone.name()),
                        over(ink, fill),
                        fill,
                        TEXT,
                    );
                }
            }
        }

        contract.assert_met();
    }
}
