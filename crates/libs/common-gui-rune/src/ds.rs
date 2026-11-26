//! The two kits (dark and light) with the shared overrides, and the surface ladder.

use crate::tokens::{
    DARK_ACCENT, DARK_BG, DARK_FG, DROP_FILL_ALPHA, LIGHT_ACCENT, LIGHT_BG, LIGHT_FG, LIGHT_MUTED,
    SUBMIT_FILL, SUBMIT_HOVER_ALPHA, SUBMIT_LABEL, SUBMIT_PRESSED_ALPHA, SUBMIT_SHEEN_ALPHA, rgb,
};
use rune::Color;
use rune_kit::{Kit, PaletteSeed};

/// The surface levels, from the window up. Content has no shadow; floating overlays add a
/// material on top of the content level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// The window background: toolbar bands, sidebars, and small windows.
    Window,
    /// The content pane.
    Content,
    /// Cards, popovers, the sheet body.
    Raised,
}

/// The two kits, built once with the overrides applied.
pub struct Ds {
    dark: Kit,
    light: Kit,
}

impl Default for Ds {
    fn default() -> Self {
        Self::new()
    }
}

impl Ds {
    pub fn new() -> Self {
        Self {
            dark: dark_kit(),
            light: light_kit(),
        }
    }

    /// The kit for the current appearance.
    pub fn kit(&self, dark: bool) -> &Kit {
        if dark { &self.dark } else { &self.light }
    }
}

/// The color of `level` in `k`'s appearance. In light mode the kit's layers step darker toward
/// `fg`, so levels are mapped by result: the window is the first darker layer and content and
/// raised surfaces are white (raised ones carry a hairline).
pub fn surface(k: &Kit, level: Surface) -> Color {
    match (k.dark, level) {
        (true, Surface::Window) => k.c.bg_base,
        (true, Surface::Content) => k.c.layer_01,
        (true, Surface::Raised) => k.c.layer_02,
        (false, Surface::Window) => k.c.layer_01,
        (false, Surface::Content | Surface::Raised) => k.c.bg_base,
    }
}

/// The fill of a drop target while a file drag hovers it: the accent at [`DROP_FILL_ALPHA`],
/// drawn over Content.
pub fn drop_fill(k: &Kit) -> Color {
    k.c.text_accent.alpha(DROP_FILL_ALPHA)
}

fn dark_kit() -> Kit {
    let mut kit = Kit::from_palette(PaletteSeed {
        bg: rgb(DARK_BG),
        fg: rgb(DARK_FG),
        accent: rgb(DARK_ACCENT),
    });
    shared_overrides(&mut kit);
    kit
}

fn light_kit() -> Kit {
    let mut kit = Kit::from_palette(PaletteSeed {
        bg: rgb(LIGHT_BG),
        fg: rgb(LIGHT_FG),
        accent: rgb(LIGHT_ACCENT),
    });
    // Muted text: the kit floors it against white only; this clears 4.5:1 on the window too.
    kit.c.text_muted = rgb(LIGHT_MUTED);
    kit.c.text_weak = rgb(LIGHT_MUTED);
    kit.c.icon_muted = rgb(LIGHT_MUTED);
    shared_overrides(&mut kit);
    kit
}

fn shared_overrides(kit: &mut Kit) {
    // Submit: one darker button blue in both appearances with a white label, a capped sheen,
    // and a hover that darkens, so the label keeps 4.5:1 in every state.
    // The kit fills Submit with `bg_icon_button_contrast` (derived as `fg`) and presses it with
    // `contrast_pressed` (a `bg` wash, white in light mode, which would brighten the blue).
    kit.c.bg_icon_button_contrast = rgb(SUBMIT_FILL);
    kit.c.submit_hover = Color::BLACK.alpha(SUBMIT_HOVER_ALPHA);
    kit.c.contrast_pressed = Color::BLACK.alpha(SUBMIT_PRESSED_ALPHA);
    kit.c.submit_fg = rgb(SUBMIT_LABEL);
    kit.c.submit_icon = rgb(SUBMIT_LABEL);
    kit.c.submit_highlight = Color::WHITE.alpha(SUBMIT_SHEEN_ALPHA);
    // Focus ring: the accent, so focus reads as the one interactive tint. The kit draws the
    // checkbox's ring and a progress bar's fill in `border_active`, a lifted neutral under 3:1
    // on every surface; the accent clears it.
    kit.c.border_focus = kit.c.text_accent;
    kit.c.border_active = kit.c.text_accent;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contrast::{Contract, LEVELS, NON_TEXT, TEXT, appearance_name, over};
    use crate::parts::BadgeTone;
    use crate::tokens::{SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XL, SPACE_XS, SPACE_XXL};

    #[test]
    fn every_shared_contract_pair_meets_its_minimum_in_both_appearances() {
        let ds = Ds::new();
        let mut contract = Contract::default();

        for dark in [true, false] {
            let k = ds.kit(dark);
            let mode = appearance_name(dark);
            for level in LEVELS {
                let bg = surface(k, level);
                for (name, fg) in [
                    ("fg", k.c.text_base),
                    ("muted", k.c.text_muted),
                    ("accent", k.c.text_accent),
                    ("danger caption", k.c.danger.fg),
                    ("warning caption", k.c.warning.fg),
                ] {
                    contract.check(&format!("{mode} {name} on {level:?}"), fg, bg, TEXT);
                }
            }

            let fill = k.c.bg_icon_button_contrast;
            for (state, face) in [
                ("rest", fill),
                ("sheen", over(k.c.submit_highlight, fill)),
                ("hover", over(k.c.submit_hover, fill)),
                ("pressed", over(k.c.contrast_pressed, fill)),
            ] {
                contract.check(
                    &format!("{mode} Submit label {state}"),
                    k.c.submit_fg,
                    face,
                    TEXT,
                );
            }
        }

        contract.assert_met();
    }

    #[test]
    fn focus_rings_meet_their_minimum_in_both_appearances() {
        let ds = Ds::new();
        let mut contract = Contract::default();

        for dark in [true, false] {
            let k = ds.kit(dark);
            let mode = appearance_name(dark);
            for level in LEVELS {
                let bg = surface(k, level);
                for (name, ring) in [
                    ("focus ring", k.c.border_focus),
                    ("checkbox focus ring", k.c.border_active),
                ] {
                    contract.check(
                        &format!("{mode} {name} on {level:?}"),
                        over(ring, bg),
                        bg,
                        NON_TEXT,
                    );
                }
            }
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

    #[test]
    fn text_and_the_accent_edge_meet_their_minimums_on_the_drop_fill() {
        let ds = Ds::new();
        let mut contract = Contract::default();

        for dark in [true, false] {
            let k = ds.kit(dark);
            let mode = appearance_name(dark);
            let fill = over(drop_fill(k), surface(k, Surface::Content));
            for (name, fg) in [
                ("fg", k.c.text_base),
                ("muted", k.c.text_muted),
                ("danger caption", k.c.danger.fg),
                ("warning caption", k.c.warning.fg),
            ] {
                contract.check(&format!("{mode} {name} on the drop fill"), fg, fill, TEXT);
            }
            contract.check(
                &format!("{mode} accent edge on the drop fill"),
                over(k.c.border_focus, fill),
                fill,
                NON_TEXT,
            );
        }

        contract.assert_met();
    }

    #[test]
    fn the_surface_ladder_rises_away_from_the_window() {
        let ds = Ds::new();
        let (dark, light) = (ds.kit(true), ds.kit(false));
        let lum = |k: &Kit, level| surface(k, level).luminance();

        assert!(lum(dark, Surface::Window) < lum(dark, Surface::Content));
        assert!(lum(dark, Surface::Content) < lum(dark, Surface::Raised));
        assert!(lum(light, Surface::Window) < lum(light, Surface::Content));
        assert_eq!(
            surface(light, Surface::Content),
            surface(light, Surface::Raised)
        );
    }

    #[test]
    fn spacing_ascends() {
        let scale = [SPACE_XS, SPACE_SM, SPACE_MD, SPACE_LG, SPACE_XL, SPACE_XXL];

        assert!(scale.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn both_appearances_share_the_submit_fill() {
        let ds = Ds::new();

        assert_eq!(
            ds.kit(true).c.bg_icon_button_contrast,
            ds.kit(false).c.bg_icon_button_contrast
        );
        assert!(ds.kit(true).dark);
        assert!(!ds.kit(false).dark);
    }
}
