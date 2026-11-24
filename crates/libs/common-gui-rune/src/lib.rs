//! The tool-neutral part of the workspace's Rune design system: the two kits with their
//! overrides, the surface ladder, tokens, motion roles, the parts the kit lacks, the system
//! appearance, and a window watch for small apps.
//!
//! A view takes a `Kit` from [`Theme::kit`] and the tokens here, and never builds a color or a
//! spring of its own. A color, size, radius, or spring literal belongs in this crate or in a
//! tool's own design module, never in a view.

pub mod appearance;
#[cfg(any(test, feature = "test-harness"))]
pub mod contrast;
mod ds;
pub mod keys;
pub mod motion;
pub mod parts;
#[cfg(any(test, feature = "test-harness"))]
pub mod test_harness;
mod theme;
pub mod tokens;
pub mod window;

pub use ds::{Ds, Surface, surface};
pub use theme::Theme;
