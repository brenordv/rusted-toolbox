//! The eh-gui design system in code: the shared kits, tokens, motion roles, and parts of
//! `common-gui-rune`, re-exported beside eh-gui's own. Views take a [`Kit`] from [`Ds::kit`]
//! and the tokens here, and never build a color or a spring of their own.
//!
//! Review rules (each is a defect):
//!
//! 1. A color, size, radius, shadow, or spring literal outside `design/` and `common-gui-rune`.
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

pub use common_gui_rune::{Ds, Surface, surface};
use rune::Color;
use rune_kit::Kit;
use tokens::{FLOATING_TINT_ALPHA_DARK, FLOATING_TINT_ALPHA_LIGHT};

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

#[cfg(test)]
mod tests;
