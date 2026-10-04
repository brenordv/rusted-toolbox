//! The workspace's egui design system: spacing, typography, and color tokens,
//! the dark theme built from them, and the component recipes every egui tool
//! composes its views from.
//!
//! Six styling rules bind every consumer of this crate:
//!
//! 1. Spacing comes only from the token scale; a layout that wants 10px
//!    actually wants `SPACE_SM` or `SPACE_MD`.
//! 2. One accent color. Status colors mark status, never decoration.
//! 3. Text gets one of the four type tokens; no ad-hoc `RichText::size`.
//! 4. Every container is `bg_base`, `bg_panel`, or `bg_raised`; depth comes
//!    from those three steps plus `border`, never from shadows.
//! 5. Monospace for anything a user might copy: keys, armored text, paths.
//! 6. Interactive affordance comes from the five globally themed widget
//!    states; recipes never hand-roll hover colors.
//!
//! A `Color32::from_rgb` or literal pixel value outside this crate is a
//! review defect.

pub mod theme;
pub mod tokens;
pub mod widgets;
