//! The eh-gui design system in code: the only place colors, sizes, radii, type, and motion are
//! defined. Views take a [`Kit`] from [`Ds::kit`] and the tokens here, and never build a color
//! or a spring of their own.
//!
//! Review rules (each is a defect):
//!
//! 1. A color, size, radius, shadow, or spring literal outside `design/`.
//! 2. An animation outside the motion roles, or a spring instead of a role.
//! 3. Any bounce; any per-row arrival animation; a number that animates more than once a second.
//! 4. More than one `Submit` button on a surface; tint on anything neither interactive nor live.
//! 5. Frosted material anywhere except the floating overlays.
//! 6. A text style outside `tokens`' type roles; a copyable value not in `TYPE_MONO`.
//! 7. File, network, or archive work in a view, a handler, or `on_close`.
//! 8. A toast for something the UI already shows; a credential reaching the clipboard.

pub mod icons;
pub mod motion;
pub mod parts;
pub mod toast;
pub mod tokens;

use rune::Color;
use rune_kit::{Kit, PaletteSeed};
use tokens::{
    DARK_ACCENT, DARK_BG, DARK_FG, FLOATING_TINT_ALPHA_DARK, FLOATING_TINT_ALPHA_LIGHT,
    LIGHT_ACCENT, LIGHT_BG, LIGHT_FG, LIGHT_MUTED, SUBMIT_FILL, SUBMIT_HOVER_ALPHA, SUBMIT_LABEL,
    SUBMIT_PRESSED_ALPHA, SUBMIT_SHEEN_ALPHA, rgb,
};

/// The surface levels, from the window up. Content has no shadow; floating overlays add a
/// material on top of the content level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// Toolbar band and sidebar.
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

/// The floating material's tint: the content surface at the appearance's alpha.
pub fn floating_tint(k: &Kit) -> Color {
    let alpha = if k.dark {
        FLOATING_TINT_ALPHA_DARK
    } else {
        FLOATING_TINT_ALPHA_LIGHT
    };
    surface(k, Surface::Content).alpha(alpha)
}

/// The fill behind a selected list row: the accent at the kit table's selection alpha.
pub fn selection_fill(k: &Kit) -> Color {
    let alpha = if k.dark {
        tokens::SELECTION_ALPHA_DARK
    } else {
        tokens::SELECTION_ALPHA_LIGHT
    };
    k.c.text_accent.alpha(alpha)
}

/// Secondary text on a selected row. Muted text sits at its 4.5:1 floor on the content
/// surface, so any selection tint pushes it under; selected rows draw it in `fg`.
pub fn selected_secondary_text(k: &Kit) -> Color {
    k.c.text_base
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
    // Focus ring: the accent, so focus reads as the one interactive tint.
    kit.c.border_focus = kit.c.text_accent;
}

#[cfg(test)]
mod tests;
