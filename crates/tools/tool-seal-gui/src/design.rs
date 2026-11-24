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

/// The kits seal-gui draws its checkboxes with. The kit draws an empty checkbox's only edge in
/// its weak border color, near 1.4:1 on the content surface; these copies draw it in the muted
/// icon color, over 5:1, and leave every other role, the hairlines' among them, as they are.
pub struct CheckboxKits {
    dark: Kit,
    light: Kit,
}

impl CheckboxKits {
    pub fn new() -> Self {
        let ds = Ds::new();
        let edged = |dark: bool| {
            let mut kit = ds.kit(dark).clone();
            kit.c.border_weak = kit.c.icon_muted;
            kit
        };
        Self {
            dark: edged(true),
            light: edged(false),
        }
    }

    /// The checkbox kit for the appearance the window draws in.
    pub fn kit(&self, dark: bool) -> &Kit {
        if dark { &self.dark } else { &self.light }
    }
}

impl Default for CheckboxKits {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common_gui_rune::contrast::{Contract, NON_TEXT, appearance_name, over};
    use common_gui_rune::{Surface, surface};

    #[test]
    fn a_checkbox_is_marked_at_3_to_1_on_the_content_surface_empty_and_checked() {
        let kits = CheckboxKits::new();
        let mut contract = Contract::default();

        for dark in [true, false] {
            let k = kits.kit(dark);
            let mode = appearance_name(dark);
            let content = surface(k, Surface::Content);
            let checked_fill = over(k.c.surface_weak, content);
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
        }

        contract.assert_met();
    }
}
