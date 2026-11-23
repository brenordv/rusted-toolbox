//! Contrast-contract helpers for tests: WCAG ratios, compositing, and a collector that reports
//! every failing pair at once.
//!
//! Test support only (`cfg(test)` or the dev-only `test-harness` feature): [`Contract::assert_met`]
//! panics on a failing pair, which fails the test that called it.

use crate::Surface;
use rune::Color;

/// The least ratio for text.
pub const TEXT: f32 = 4.5;
/// The least ratio for non-text marks: icons, rings, dots.
pub const NON_TEXT: f32 = 3.0;
/// Every surface level.
pub const LEVELS: [Surface; 3] = [Surface::Window, Surface::Content, Surface::Raised];

/// WCAG contrast ratio; `Color` holds linear light, so `luminance` is relative luminance.
pub fn contrast(a: Color, b: Color) -> f32 {
    let (a, b) = (a.luminance(), b.luminance());
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `top` (straight alpha) composited over opaque `bottom`.
pub fn over(top: Color, bottom: Color) -> Color {
    let blend = |t: f32, b: f32| t * top.a + b * (1.0 - top.a);
    Color {
        r: blend(top.r, bottom.r),
        g: blend(top.g, bottom.g),
        b: blend(top.b, bottom.b),
        a: 1.0,
    }
}

/// "dark" or "light", for a pair's name.
pub fn appearance_name(dark: bool) -> &'static str {
    if dark { "dark" } else { "light" }
}

/// The pairs that fell under their minimum.
#[derive(Debug, Default)]
pub struct Contract {
    pub failures: Vec<String>,
}

impl Contract {
    /// Records `name` as a failure when `fg` on `bg` is under `minimum`.
    pub fn check(&mut self, name: &str, fg: Color, bg: Color, minimum: f32) {
        let ratio = contrast(fg, bg);
        if ratio < minimum {
            self.failures
                .push(format!("{name}: {ratio:.2}:1 (needs {minimum}:1)"));
        }
    }

    /// # Panics
    /// When a checked pair failed, listing every one.
    pub fn assert_met(&self) {
        assert!(
            self.failures.is_empty(),
            "contrast contract failures:\n{}",
            self.failures.join("\n")
        );
    }
}
